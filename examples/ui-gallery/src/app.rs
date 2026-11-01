//! corten-ui's components on one page: a seller's draft listing for one card.
//!
//! | Route | |
//! |---|---|
//! | `GET /?tab=offers` | the gallery, with that tab selected |
//! | `GET /?sale=last` | the gallery with the last sale's sheet open beside it |
//! | `POST /sell` | the listing form: a patch with JavaScript, the page without; refused (422) when it ships nowhere or takes offers with no lowest one |
//! | `POST /archive`, `POST /watch` | the menu's form actions: archive the draft, watch the card |
//! | `POST /save` | saving the draft, answered with a toast |
//! | `POST /rename` | the rename dialog's form: refused, the dialog drawn open; else back to the draft, whose toaster says it was renamed (`req.redirect_with_notice`) |
//! | `GET /help` | the menu's link |
//! | `GET /orders?page=2` | the seller's orders, a page of them: the kit's other components |
//! | `POST /orders/settings` | the shipping settings: back to the orders, whose toaster says they were saved |
use corten::{Component, i18n::Currency, prelude::*};
use serde::Deserialize;
use std::time::{Duration, SystemTime};

use corten_ui::{Toast, Toaster};

use crate::gallery::{
    CARD, Condition, Gallery, MarketTab, MenuResult, OrderResult, Period, Pricing, Region, RenameDialog, Sale, Times,
};
use crate::orders::{Orders, PAGES, SettingsForm};
use crate::ui::toasts::Toasts;

corten::i18n::messages! {
    /// The gallery's text: one locale, for the relative times.
    pub struct Msg { "en" => "locales/en.json" }
}

/// How far the best offer's expiry is ahead of each page, and the listing behind it.
pub const EXPIRES_IN: Duration = Duration::from_secs(90);
pub const LISTED_AGO: Duration = Duration::from_secs(3 * 3600 + 5 * 60);
/// When the draft was created, after the Unix epoch: 2026-03-22T15:04:05Z, written in the
/// visitor's zone.
pub const CREATED: Duration = Duration::from_secs(1_774_191_845);

corten::assets::embed! {
    /// The files in `static/`, compiled into the program: `Static::STYLE_CSS` is `static/style.css`.
    pub struct Static = "static";
}

/// Every URL the gallery serves and links to, with the handlers that serve it.
#[corten::routes]
pub mod url {
    #[route("/", get = home)]
    pub struct Home {
        pub tab: Option<crate::gallery::MarketTab>,
        pub sale: Option<crate::gallery::Sale>,
        pub period: Option<crate::gallery::Period>,
    }

    #[route("/sell", post = sell)]
    pub struct Sell;

    #[route("/help", get = help)]
    pub struct Help;

    #[route("/archive", post = archive)]
    pub struct Archive;

    #[route("/watch", post = watch)]
    pub struct Watch;

    #[route("/save", post = save)]
    pub struct Save;

    #[route("/rename", post = rename)]
    pub struct Rename;

    #[route("/orders", get = orders)]
    pub struct Orders {
        pub page: Option<usize>,
    }

    #[route("/orders/settings", post = settings)]
    pub struct Settings;
}

/// The app's components: the kit's (`ui::COMPONENTS`) are listed before them, corten-ui's after.
pub const COMPONENTS: &[Component] = &[Gallery, crate::gallery::Sku, Orders];

pub fn app() -> App {
    App::new(url::router())
        .assets(Static)
        .components(crate::ui::COMPONENTS)
        .components(COMPONENTS)
        .components(corten_ui::COMPONENTS)
        .zoned_dates()
}

/// The document around every page, with the toaster: what an answer says (`toast`) or, after a
/// redirect, what the write said (`req.notice()`), which a write's redirect draws again with the
/// rest of the layout.
#[component]
fn Layout<'a>(req: &'a Req, title: &'a str, toast: Option<&'a str>, children: impl Render) -> impl Document {
    let notice = req.notice();
    let message = notice.map(|n| n as &dyn Render).or(toast.as_ref().map(|t| t as &dyn Render));
    html! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <link rel="stylesheet" href={req.asset(Static::STYLE_CSS)}>
                <link rel="icon" type="image/svg+xml" href={req.asset(Static::FAVICON_SVG)}>
                <title>{title}</title>
                <Corten />
            </head>
            <body>
                <main>{children}</main>
                <Toasts><Toaster close="Dismiss" seconds={2} toast={Toast { message }} /></Toasts>
            </body>
        </html>
    }
}

/// The offer's relative times, as of now.
fn times(req: &Req) -> Times {
    let (t, now): (Msg, _) = (req.t(), SystemTime::now());
    Times { t, now }
}

/// The gallery in the layout.
fn gallery(req: &Req, gallery: Gallery<'_>) -> HtmlResult {
    req.page(Layout { req, title: TITLE, toast: None, children: gallery })
}

const TITLE: &str = "Sell Ashen Drake | corten-ui";

/// The listings tab with nothing to report: what each answer below changes one part of
/// (`Gallery { result: &text, ..listings(&req, &none) }`), since every prop is written.
fn listings<'a>(req: &Req, none: &'a Errors) -> Gallery<'a> {
    Gallery {
        tab: MarketTab::Listings,
        period: Period::Month,
        result: "",
        sale: false,
        times: times(req),
        rename: RenameDialog { name: "", errors: none },
    }
}

async fn home(req: Req, Params(q): Params<url::Home>) -> HtmlResult {
    let none = Errors::new();
    let tab = q.tab.unwrap_or(MarketTab::Listings);
    let period = q.period.unwrap_or(Period::Month);
    gallery(&req, Gallery { tab, period, sale: q.sale == Some(Sale::Last), ..listings(&req, &none) })
}

#[derive(Deserialize, Fields)]
pub struct SellForm {
    /// A count: a fraction does not read, and the browser's `step` stops one before it is sent.
    pub qty: u32,
    /// Money as the reader typed it, read by `t.parse_money`: a refused entry is shown back as is.
    pub price: String,
    pub weight: f64,
    /// One of the combo box's options: a word outside them is refused, as a malformed number is.
    pub condition: Condition,
    /// The regions the seller ships to: at least one (`CheckboxGroup`).
    #[serde(default)]
    pub ships: Vec<Region>,
    /// A fixed price, or offers with the lowest offer the seller accepts (`RequiredWhen`).
    pub pricing: Pricing,
    /// Money too, empty unless the listing takes offers.
    #[serde(default)]
    pub floor: String,
}

/// The listing's currency.
pub const USD: Currency = Currency::new("USD");

/// What a listing that ships nowhere is told, by the browser and by the handler.
pub const NO_REGION: &str = "Choose where you ship.";

/// Listed, or refused when it ships nowhere or takes offers with no lowest one: the browser stops
/// both before sending when its script runs, and this is the check without it.
async fn sell(req: Req, Form(form): Form<SellForm>) -> HtmlResult {
    let f = SellForm::FIELDS;
    let t: Msg = req.t();
    let mut errors = Errors::new();
    let price = t.parse_money(&form.price, USD);
    if price.is_none() {
        errors.add(f.price, "Enter a price, such as 12.50.");
    }
    if form.ships.is_empty() {
        errors.add(f.ships, NO_REGION);
    }
    if form.pricing == Pricing::Offers && t.parse_money(&form.floor, USD).is_none() {
        errors.add(f.floor, "Enter the lowest offer you accept.");
    }
    if let Some((_, message)) = errors.iter().next() {
        let none = Errors::new();
        let page = || async { gallery(&req, Gallery { result: message, ..listings(&req, &none) }) };
        return req.invalid(&errors, OrderResult { text: message }, page).await;
    }
    let condition = form.condition.label();
    // Some here: a price that does not read is refused above.
    let price = price.map(|cents| t.money(cents, USD).to_string()).unwrap_or_default();
    let text = format!("Listed {} {condition} {CARD} at {price} each, {:.1} oz to ship.", form.qty, form.weight);
    let none = Errors::new();
    let page = || async { gallery(&req, Gallery { result: &text, ..listings(&req, &none) }) };
    req.patch_or(OrderResult { text: &text }, page).await
}

/// Answers with a toast: the message as a patch, or in the page without JavaScript.
async fn save(req: Req) -> HtmlResult {
    const SAVED: &str = "Draft saved.";
    let none = Errors::new();
    let page = || async { req.page(Layout { req: &req, title: TITLE, toast: Some(SAVED), children: listings(&req, &none) }) };
    req.patch_or(Toast { message: Some(SAVED) }, page).await
}

#[derive(Deserialize, Fields)]
pub struct RenameForm {
    pub name: String,
}

/// A name is required: refused, the dialog comes back drawn open with the error, as a patch or in
/// the whole page.
async fn rename(req: Req, Form(form): Form<RenameForm>) -> HtmlResult {
    let name = form.name.trim();
    if !name.is_empty() {
        return req.redirect_with_notice(&url::Home { tab: None, sale: None, period: None }, format_args!("Renamed to {name}."));
    }
    let mut errors = Errors::new();
    errors.add(RenameForm::FIELDS.name, "Give the draft a name.");
    let page = || async { gallery(&req, Gallery { rename: RenameDialog { name, errors: &errors }, ..listings(&req, &errors) }) };
    req.invalid(&errors, RenameDialog { name, errors: &errors }, page).await
}

async fn archive(req: Req) -> HtmlResult {
    menu_action(req, "Archived.").await
}

async fn watch(req: Req) -> HtmlResult {
    menu_action(req, "Watching.").await
}

async fn menu_action(req: Req, text: &'static str) -> HtmlResult {
    req.patch_or(MenuResult { text }, || async { req.redirect(&url::Home { tab: None, sale: None, period: None }) }).await
}

/// A page of the orders: one past the last is the last.
async fn orders(req: Req, Params(q): Params<url::Orders>) -> HtmlResult {
    let page = q.page.unwrap_or(1).clamp(1, PAGES);
    // The avatar takes the address as text.
    let (t, photo) = (req.t(), req.asset(Static::FAVICON_SVG).to_string());
    req.page(html! {
        <Layout req={&req} title="Orders | corten-ui" toast={None}>
            <Orders t={t} page={page} photo={&photo} />
        </Layout>
    })
}

/// Saved, back to the orders, whose toaster says so.
async fn settings(req: Req, Form(form): Form<SettingsForm>) -> HtmlResult {
    let (from, days) = (form.warehouse.label(), form.handling.clamp(1, 5));
    let emails = if form.notify { "on" } else { "off" };
    let note = if form.note.trim().is_empty() { "no note" } else { "your note" };
    req.redirect_with_notice(
        &url::Orders { page: None },
        format_args!("Saved: ships from {from} in {days} days with {note}; shipping emails {emails}."),
    )
}

async fn help(req: Req) -> HtmlResult {
    req.page(html! {
        <Layout req={&req} title="Help | corten-ui" toast={None}>
            <h1>"Listing a card"</h1>
            <p>"A draft is yours until you list it. Each part of the form:"</p>
            <dl>
                <dt>"Price and lowest offer"</dt>
                <dd>
                    "In dollars and cents, as you would write them: 12.50 or $1,299.50. Take offers and buyers can offer less, down to the lowest you set."
                </dd>
                <dt>"Condition"</dt>
                <dd>"Grade the copy you are selling, from Near Mint (as it left the pack) to Damaged. Type to find one."</dd>
                <dt>"Ships to"</dt>
                <dd>"Choose at least one region. Shipping is worked out from the weight you enter."</dd>
                <dt>"Archive and Watch"</dt>
                <dd>"Archiving puts the draft away without listing it. Watching tells you when this card's market low moves."</dd>
            </dl>
            <p><a href={url::Home { tab: None, sale: None, period: None }}>"Back to the draft"</a></p>
        </Layout>
    })
}

#[cfg(test)]
mod tests {
    use corten::axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;

    async fn send(req: Request<Body>) -> String {
        let res = app().router().oneshot(req).await.unwrap();
        String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
    }

    /// The scripts are checked against these declarations by `npm run check`.
    #[test]
    fn script_types_match_the_server() {
        corten::types::Types::new()
            .components(COMPONENTS)
            .components(corten_ui::COMPONENTS)
            .sync(concat!(env!("CARGO_MANIFEST_DIR"), "/types/app.d.ts"));
    }

    #[tokio::test]
    async fn the_page_preloads_the_scripts_it_uses_and_selects_the_tab_in_the_url() {
        let page = send(Request::get("/?tab=offers").body(Body::empty()).unwrap()).await;
        for name in ["ui.number_field", "ui.combo_box", "ui.tab_group", "ui.menu_button", "ui.toaster", "ui.dialog"] {
            assert!(page.contains(&format!("/scripts/{name}.js\">")), "{name} is preloaded: {page}");
        }
        assert!(
            page.contains(r#"id="book-tab-1" href="/?tab=offers" aria-controls="book-panel-1" aria-selected="true""#),
            "{page}"
        );
    }

    #[tokio::test]
    async fn the_paragraph_listens_for_the_event_the_copy_button_dispatches() {
        // A library's event carries its prefix; the listener names the event by its type, so the
        // two cannot drift apart.
        let page = send(Request::get("/").body(Body::empty()).unwrap()).await;
        assert!(page.contains(r#"data-on="ui-copied:sku.counted($e,@copied,@copies)""#), "{page}");
    }

    #[tokio::test]
    async fn the_form_answers_with_a_patch_or_the_page() {
        let form = || Request::post("/sell").header("content-type", "application/x-www-form-urlencoded");
        const LISTED: &str = "qty=2&price=12.50&weight=0.5&condition=lp&ships=us&ships=eu&pricing=fixed&floor=";
        let patch = send(form().header("corten", "action").body(Body::from(LISTED)).unwrap()).await;
        assert_eq!(
            patch,
            r#"<p id="result" role="status">Listed 2 Lightly Played Ashen Drake at $12.50 each, 0.5 oz to ship.</p>"#
        );
        let page = send(form().body(Body::from(LISTED)).unwrap()).await;
        assert!(page.contains("<html") && page.contains("Listed 2 Lightly Played Ashen Drake"));
    }

    /// What the browser stops when its script runs (no region, offers with no lowest one), the
    /// handler refuses without it.
    #[tokio::test]
    async fn a_listing_that_ships_nowhere_or_takes_offers_without_a_floor_is_refused() {
        let post = |body: &'static str| {
            app().router().oneshot(
                Request::post("/sell")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .header("corten", "action")
                    .body(Body::from(body))
                    .unwrap(),
            )
        };
        for (body, message) in [
            ("qty=1&price=1&weight=1&condition=lp&pricing=fixed&floor=", "Choose where you ship."),
            ("qty=1&price=1&weight=1&condition=lp&ships=ca&pricing=offers&floor=", "Enter the lowest offer you accept."),
            ("qty=1&price=1.2.3&weight=1&condition=lp&ships=ca&pricing=fixed&floor=", "Enter a price, such as 12.50."),
        ] {
            let res = post(body).await.unwrap();
            assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY, "{body}");
            let text = String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
            assert_eq!(text, format!(r#"<p id="result" role="status">{message}</p>"#));
        }
        let res = post("qty=1&price=%241%2C299.50&weight=1&condition=lp&ships=ca&pricing=offers&floor=5").await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    /// The choices are enums: a word the page does not offer (a forged post) is refused before the
    /// handler runs, as a malformed number is.
    #[tokio::test]
    async fn a_word_the_page_does_not_offer_is_refused() {
        for body in [
            "qty=1&price=1&weight=1&condition=mint&ships=us&pricing=fixed&floor=",
            "qty=1&price=1&weight=1&condition=lp&ships=mars&pricing=fixed&floor=",
            "qty=1&price=1&weight=1&condition=lp&ships=us&pricing=auction&floor=",
        ] {
            let res = app()
                .router()
                .oneshot(
                    Request::post("/sell")
                        .header("content-type", "application/x-www-form-urlencoded")
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        }
        // An address with a tab the page does not have (an old link, a renamed tab) reads as no tab:
        // the page opens on its first, where a posted word it does not offer is refused above.
        let tab = app().router().oneshot(Request::get("/?tab=bids").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(tab.status(), StatusCode::OK, "a tab the page does not have");
    }

    #[tokio::test]
    async fn the_address_opens_the_sale_sheet_and_its_link_closes_it() {
        let open = send(Request::get("/?sale=last").body(Body::empty()).unwrap()).await;
        assert!(
            open.contains(r#"<dialog data-scope="sale-sheet" open aria-label="Last sale" data-on="connect:ui.dialog.modalClosedBy($t,@close,$s)">"#),
            "{open}"
        );
        assert!(open.contains(r#"<a data-ref="close" href="/">Close</a>"#), "{open}");
        let home = send(Request::get("/").body(Body::empty()).unwrap()).await;
        assert!(!home.contains("Last sale\""), "drawn only when the address names it: {home}");
        assert!(home.contains(r#"<a href="/?sale=last">Last sale</a>"#), "{home}");
    }

    #[tokio::test]
    async fn a_refused_rename_draws_the_dialog_open() {
        let rename = || Request::post("/rename").header("content-type", "application/x-www-form-urlencoded");
        let patch = send(rename().header("corten", "action").body(Body::from("name=+")).unwrap()).await;
        assert!(
            patch.starts_with(r#"<dialog id="rename" open aria-label="Rename the draft" data-on="connect:ui.dialog.modal($t)">"#),
            "{patch}"
        );
        assert!(patch.contains("aria-invalid=\"true\"") && patch.contains("Give the draft a name."), "{patch}");
        let page = send(rename().body(Body::from("name=")).unwrap()).await;
        assert!(page.contains("<html") && page.contains(r#"<dialog id="rename" open"#), "{page}");
        let home = send(Request::get("/").body(Body::empty()).unwrap()).await;
        assert!(home.contains(r#"<dialog id="rename" aria-label"#), "drawn closed: {home}");
    }

    /// Each page's links are typed routes: the first page's address has no number, and a number
    /// past the last reads as the last.
    #[tokio::test]
    async fn the_orders_link_to_their_pages() {
        let first = send(Request::get("/orders").body(Body::empty()).unwrap()).await;
        assert!(first.contains(r#"<a class="#) && first.contains(r#"href="/orders?page=2" rel="next""#), "{first}");
        assert!(first.contains(r#"href="/orders" aria-current="page">1</a>"#), "{first}");
        assert!(first.contains(">AL<") && !first.contains(">BL<"), "four to a page: {first}");
        let last = send(Request::get("/orders?page=9").body(Body::empty()).unwrap()).await;
        assert!(last.contains(r#"href="/orders?page=3" aria-current="page">3</a>"#), "{last}");
        assert!(last.contains(">JB<") && !last.contains(">AL<"), "{last}");
    }

    #[tokio::test]
    async fn saved_settings_go_back_to_the_orders_with_a_notice() {
        let res = app()
            .router()
            .oneshot(
                Request::post("/orders/settings")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from("warehouse=austin&handling=3&note="))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(res.headers()["location"], "/orders");
        let forged = app()
            .router()
            .oneshot(
                Request::post("/orders/settings")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from("warehouse=mars&handling=3"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(forged.status(), StatusCode::UNPROCESSABLE_ENTITY, "a warehouse the page does not offer");
    }

    #[tokio::test]
    async fn saving_answers_with_a_toast() {
        let save = || Request::post("/save");
        let patch = send(save().header("corten", "action").body(Body::empty()).unwrap()).await;
        assert_eq!(patch, r#"<div id="ui-toast">Draft saved.</div>"#);
        let page = send(save().body(Body::empty()).unwrap()).await;
        assert!(page.contains(r#"<div data-ref="status" role="status"><div id="ui-toast">Draft saved.</div></div>"#), "{page}");
        let home = send(Request::get("/").body(Body::empty()).unwrap()).await;
        assert!(home.contains(r#"<div id="ui-toast" hidden></div>"#), "{home}");
    }
}
