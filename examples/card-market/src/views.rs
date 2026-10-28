//! The app's views. Each component's stylesheet and script are next to it in `views/`.
use corten::{Component, prelude::*};

use std::sync::Arc;

use crate::{model::ItemView, routes, source::Shelf, text::Msg, theme};

mod alert;
mod item_card;
mod item_detail;
mod item_list;
mod listing_sort;
mod offer_list;
mod price;
mod site_header;

pub use alert::{AlertForm, AlertFormView, AlertMessage};
pub use item_card::ItemCard;
pub use item_detail::{ItemDetail, ItemPage};
pub use item_list::ItemList;
pub use listing_sort::ListingSort;
pub use offer_list::OfferList;
pub use price::Price;
pub use site_header::SiteHeader;

/// Every component with a script or a stylesheet, tokens first: for `App::components` and the
/// generated types.
pub const COMPONENTS: &[Component] =
    &[theme::TOKENS, SiteHeader, ItemDetail, AlertForm, Price, ItemList, ItemCard, ListingSort, OfferList];

/// The page for a failed request (`App::error_page`), in the layout; under `corten dev` it shows
/// the error. The PWA's offline page is `Failure::Offline`.
pub async fn error_page(failure: Failure, req: Req) -> HtmlResult {
    let t: Msg = req.t();
    let (title, text) = match failure {
        Failure::NotFound => (t.error_not_found(), t.error_not_found_text()),
        Failure::TimedOut | Failure::Offline | Failure::Unidentified { .. } => {
            (t.error_unavailable(), t.error_unavailable_text())
        }
        Failure::WrongMethod
        | Failure::CrossSite
        | Failure::Refused { .. }
        | Failure::Rejected { .. }
        | Failure::Failed { .. }
        | Failure::Panicked { .. } => (t.error_failed(), t.error_failed_text()),
    };
    req.page(html! {
        <Layout req={&req} title={title} head="">
            <h1>{title}</h1>
            <p>{text}</p>
            <p><a href={routes::Home}>{t.error_home()}</a></p>
            if let Some(detail) = failure.detail() {
                <pre>{detail}</pre>
            }
        </Layout>
    })
}

pub fn category<'a>(t: Msg, title: &'a str, items: &'a [Arc<ItemView>]) -> impl Render + 'a {
    html! {
        <h1>{title}</h1>
        if !items.is_empty() { <ItemList t={t} shelf={Shelf::Items(items)} /> }
        <p><a href={routes::Home}>{t.category_back()}</a></p>
    }
}

/// The document around every page: `title`, the page's own `head` tags and the page itself
/// (`children`). A client navigation gets only the title, the head tags and `<main>`'s children.
/// The app's stylesheet comes before `<Corten />`, which links the components' stylesheet, so a
/// component's rules win ties with element defaults.
#[component]
pub fn Layout(req: &Req, title: &str, head: impl Render, children: impl Render) -> impl Document {
    html! {
        <!DOCTYPE html>
        <html lang={req.lang()}>
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <link rel="stylesheet" href={req.asset("style.css")}>
                <link rel="icon" type="image/svg+xml" href={req.asset("favicon.svg")}>
                <title>{title}" | Card Market"</title>
                {head}
                <Corten />
                <script type="module" src={req.asset("app.js")}></script>
            </head>
            <body>
                <SiteHeader />
                <main>{children}</main>
            </body>
        </html>
    }
}
