//! corten-ui's components on one page: a seller's draft listing for one card.
//!
//! | Route | |
//! |---|---|
//! | `GET /?tab=offers` | the gallery, with that tab selected |
//! | `GET /?sale=last` | the gallery with the last sale's sheet open beside it |
//! | `POST /sell` | the listing form: a patch with JavaScript, the page without; refused (422) when it ships nowhere or takes offers with no lowest one |
//! | `POST /archive`, `POST /watch` | the menu's form actions: archive the draft, watch the card |
//! | `POST /save` | saving the draft, answered with a toast |
//! | `POST /rename` | the rename dialog's form: refused, the dialog drawn open; else back to the draft |
//! | `GET /help` | the menu's link |
use corten::{Component, prelude::*};
use serde::Deserialize;

use corten_ui::Toast;

use crate::gallery::{CARD, CONDITIONS, Gallery, LAST_SALE, MenuResult, OrderResult, RenameDialog, TABS};

/// Every URL the gallery serves and links to, with the handlers that serve it.
#[corten::routes]
pub mod url {
    #[route("/", get = home)]
    pub struct Home<'a> {
        pub tab: Option<&'a str>,
        pub sale: Option<&'a str>,
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
}

/// The app's components, then corten-ui's.
pub const COMPONENTS: &[Component] = &[crate::field::Field, Gallery, crate::gallery::Sku];

pub fn app() -> App {
    App::new(url::router()).components(COMPONENTS).components(corten_ui::COMPONENTS)
}

/// The document around every page.
#[component]
fn Layout(req: &Req, title: &str, children: impl Render) -> impl Document {
    html! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <link rel="stylesheet" href={req.asset("style.css")}>
                <link rel="icon" type="image/svg+xml" href={req.asset("favicon.svg")}>
                <title>{title}</title>
                <Corten />
            </head>
            <body>
                <main>{children}</main>
            </body>
        </html>
    }
}

/// The gallery in the layout.
fn gallery(req: &Req, gallery: Gallery<'_>) -> HtmlResult {
    req.page(Layout { req, title: TITLE, children: gallery })
}

#[derive(Deserialize)]
struct HomeQuery {
    tab: Option<String>,
    sale: Option<String>,
}

const TITLE: &str = "Sell Ashen Drake | corten-ui";

async fn home(req: Req, Query(q): Query<HomeQuery>) -> HtmlResult {
    let tab = TABS.iter().position(|t| q.tab.as_deref() == Some(*t)).unwrap_or(0);
    let sale = q.sale.as_deref() == Some(LAST_SALE);
    gallery(&req, Gallery { tab, result: "", toast: None, sale, rename: RenameDialog { name: "", errors: &Errors::new() } })
}

#[derive(Deserialize, Fields)]
pub struct SellForm {
    pub qty: f64,
    pub price: f64,
    pub condition: String,
    /// The regions the seller ships to: at least one (`CheckboxGroup`).
    #[serde(default)]
    pub ships: Vec<String>,
    /// `fixed`, or `offers` with the lowest offer the seller accepts (`RequiredWhen`).
    pub pricing: String,
    pub floor: Option<f64>,
}

/// The pricing that takes offers, and needs the lowest one.
pub const OFFERS: &str = "offers";

/// What a listing that ships nowhere is told, by the browser and by the handler.
pub const NO_REGION: &str = "Choose where you ship.";

/// Listed, or refused when it ships nowhere or takes offers with no lowest one: the browser stops
/// both before sending when its script runs, and this is the check without it.
async fn sell(req: Req, Form(form): Form<SellForm>) -> HtmlResult {
    let f = SellForm::FIELDS;
    let mut errors = Errors::new();
    if form.ships.is_empty() {
        errors.add(f.ships, NO_REGION);
    }
    if form.pricing == OFFERS && form.floor.is_none() {
        errors.add(f.floor, "Enter the lowest offer you accept.");
    }
    if let Some((_, message)) = errors.iter().next() {
        let page = || async {
            gallery(
                &req,
                Gallery {
                    tab: 0,
                    result: message,
                    toast: None,
                    sale: false,
                    rename: RenameDialog { name: "", errors: &Errors::new() },
                },
            )
        };
        return req.invalid(&errors, OrderResult { text: message }, page).await;
    }
    let condition = CONDITIONS.iter().find(|c| c.value == form.condition).map_or("ungraded", |c| c.label);
    let text = format!("Listed {} {condition} {CARD} at ${:.2} each.", form.qty, form.price);
    let page = || async {
        gallery(
            &req,
            Gallery {
                tab: 0,
                result: &text,
                toast: None,
                sale: false,
                rename: RenameDialog { name: "", errors: &Errors::new() },
            },
        )
    };
    req.patch_or(OrderResult { text: &text }, page).await
}

/// Answers with a toast: the message as a patch, or in the page without JavaScript.
async fn save(req: Req) -> HtmlResult {
    const SAVED: &str = "Draft saved.";
    let page = || async {
        gallery(
            &req,
            Gallery {
                tab: 0,
                result: "",
                toast: Some(SAVED),
                sale: false,
                rename: RenameDialog { name: "", errors: &Errors::new() },
            },
        )
    };
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
        return req.redirect(&url::Home { tab: None, sale: None });
    }
    let mut errors = Errors::new();
    errors.add(RenameForm::FIELDS.name, "Give the draft a name.");
    let page = || async {
        gallery(&req, Gallery { tab: 0, result: "", toast: None, sale: false, rename: RenameDialog { name, errors: &errors } })
    };
    req.invalid(&errors, RenameDialog { name, errors: &errors }, page).await
}

async fn archive(req: Req) -> HtmlResult {
    menu_action(req, "Archived.").await
}

async fn watch(req: Req) -> HtmlResult {
    menu_action(req, "Watching.").await
}

async fn menu_action(req: Req, text: &'static str) -> HtmlResult {
    req.patch_or(MenuResult { text }, || async { req.redirect(&url::Home { tab: None, sale: None }) }).await
}

async fn help(req: Req) -> HtmlResult {
    req.page(html! {
        <Layout req={&req} title="Help | corten-ui">
            <h1>"Help"</h1>
            <p><a href={url::Home { tab: None, sale: None }}>"Back"</a></p>
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
        assert!(page.contains(r#"data-on="ui-copied:sku.counted($e,@copies)""#), "{page}");
    }

    #[tokio::test]
    async fn the_form_answers_with_a_patch_or_the_page() {
        let form = || Request::post("/sell").header("content-type", "application/x-www-form-urlencoded");
        const LISTED: &str = "qty=2&price=12.50&condition=lp&ships=us&ships=eu&pricing=fixed&floor=";
        let patch = send(form().header("corten", "action").body(Body::from(LISTED)).unwrap()).await;
        assert_eq!(patch, r#"<p id="result" role="status">Listed 2 Lightly Played Ashen Drake at $12.50 each.</p>"#);
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
            ("qty=1&price=1&condition=lp&pricing=fixed&floor=", "Choose where you ship."),
            ("qty=1&price=1&condition=lp&ships=ca&pricing=offers&floor=", "Enter the lowest offer you accept."),
        ] {
            let res = post(body).await.unwrap();
            assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY, "{body}");
            let text = String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
            assert_eq!(text, format!(r#"<p id="result" role="status">{message}</p>"#));
        }
        let res = post("qty=1&price=1&condition=lp&ships=ca&pricing=offers&floor=5").await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
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
