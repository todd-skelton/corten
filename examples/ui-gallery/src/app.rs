//! corten-ui's components on one page, with the form they belong to.
//!
//! | Route | |
//! |---|---|
//! | `GET /?tab=offers` | the gallery, with that tab selected |
//! | `POST /order` | the form: a patch with JavaScript, the page without |
//! | `POST /archive`, `POST /watch` | the menu's form actions |
//! | `POST /save` | a form that answers with a toast |
//! | `GET /help` | the menu's link |
use corten::{Component, prelude::*};
use serde::Deserialize;

use corten_ui::Toast;

use crate::gallery::{COLORS, Gallery, MenuResult, OrderResult, TABS};

pub mod url {
    use corten::Route;

    #[derive(Route)]
    #[route("/")]
    pub struct Home<'a> {
        pub tab: Option<&'a str>,
    }

    #[derive(Route)]
    #[route("/order")]
    pub struct Order;

    #[derive(Route)]
    #[route("/help")]
    pub struct Help;

    #[derive(Route)]
    #[route("/archive")]
    pub struct Archive;

    #[derive(Route)]
    #[route("/watch")]
    pub struct Watch;

    #[derive(Route)]
    #[route("/save")]
    pub struct Save;
}

/// The app's components, then corten-ui's.
pub const COMPONENTS: &[Component] = &[crate::field::Field, Gallery];

pub fn app() -> App {
    let routes = Router::new()
        .route(url::Home::PATH, get(home))
        .route(url::Order::PATH, post(order))
        .route(url::Help::PATH, get(help))
        .route(url::Archive::PATH, post(|req: Req| menu_action(req, "Archived.")))
        .route(url::Watch::PATH, post(|req: Req| menu_action(req, "Watching.")))
        .route(url::Save::PATH, post(save));
    App::new(routes).components(COMPONENTS).components(corten_ui::COMPONENTS)
}

#[derive(Deserialize)]
struct HomeQuery {
    tab: Option<String>,
}

const TITLE: &str = "corten-ui";

async fn home(req: Req, Query(q): Query<HomeQuery>) -> corten::Result {
    let tab = TABS.iter().position(|t| q.tab.as_deref() == Some(*t)).unwrap_or(0);
    req.page(TITLE, Gallery { tab, result: "", toast: None })
}

#[derive(Deserialize, Fields)]
pub struct OrderForm {
    pub qty: f64,
    pub price: f64,
    pub color: String,
}

async fn order(req: Req, Form(form): Form<OrderForm>) -> corten::Result {
    let color = COLORS.iter().find(|c| c.value == form.color).map_or("no color", |c| c.label);
    let text = format!("Ordered {} in {color} at ${:.2}.", form.qty, form.price);
    match req.mode {
        Mode::Action => req.patch(OrderResult { text: &text }),
        _ => req.page(TITLE, Gallery { tab: 0, result: &text, toast: None }),
    }
}

/// Answers with a toast: the message as a patch, or in the page without JavaScript.
async fn save(req: Req) -> corten::Result {
    const SAVED: &str = "Draft saved.";
    match req.mode {
        Mode::Action => req.patch(Toast { message: Some(&SAVED) }),
        _ => req.page(TITLE, Gallery { tab: 0, result: "", toast: Some(SAVED) }),
    }
}

async fn menu_action(req: Req, text: &'static str) -> corten::Result {
    match req.mode {
        Mode::Action => req.patch(MenuResult { text }),
        _ => req.redirect(&url::Home { tab: None }),
    }
}

async fn help(req: Req) -> corten::Result {
    req.page(
        "Help | corten-ui",
        html! {
            <h1>"Help"</h1>
            <p><a href={url::Home { tab: None }}>"Back"</a></p>
        },
    )
}

#[cfg(test)]
mod tests {
    use corten::axum::{body::Body, http::Request};
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
        for name in ["ui.number_field", "ui.combo_box", "ui.tab_group", "ui.menu_button", "ui.toaster"] {
            assert!(page.contains(&format!("/scripts/{name}.js\">")), "{name} is preloaded: {page}");
        }
        assert!(
            page.contains(r#"id="book-tab-1" href="/?tab=offers" aria-controls="book-panel-1" aria-selected="true""#),
            "{page}"
        );
    }

    #[tokio::test]
    async fn the_form_answers_with_a_patch_or_the_page() {
        let form = || Request::post("/order").header("content-type", "application/x-www-form-urlencoded");
        let patch = send(form().header("corten", "action").body(Body::from("qty=2&price=12.50&color=sand")).unwrap()).await;
        assert_eq!(patch, r#"<p id="result" role="status">Ordered 2 in Desert Sand at $12.50.</p>"#);
        let page = send(form().body(Body::from("qty=2&price=12.50&color=sand")).unwrap()).await;
        assert!(page.contains("<html") && page.contains("Ordered 2 in Desert Sand"));
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
