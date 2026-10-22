//! The app's views. Each component's stylesheet and script are next to it in `views/`.
use corten::{Component, prelude::*};

use crate::{routes, text::Msg, theme};

mod alert;
mod home;
mod item_detail;
mod listing_sort;
mod offer_list;
mod price;

pub use alert::{AlertForm, AlertFormView, AlertMessage};
pub use home::Home;
pub use item_detail::{ItemDetail, ItemPage};
pub use listing_sort::ListingSort;
pub use offer_list::OfferList;
pub use price::Price;

/// Every component with a script or a stylesheet, tokens first: for `App::components` and the
/// generated types.
pub const COMPONENTS: &[Component] = &[theme::TOKENS, ItemDetail, AlertForm, Price, Home, ListingSort, OfferList];

pub fn category(t: Msg, slug: &str) -> impl Render + '_ {
    html! {
        <h1>{t.category_title(slug)}</h1>
        <p><a href={routes::Home}>{t.category_back()}</a></p>
    }
}

/// The document around every page. The app's stylesheet comes before `{doc.head}`, which links
/// the components' stylesheet, so a component's rules win ties with element defaults.
pub fn shell(doc: Document) -> impl Render {
    html! {
        <!DOCTYPE html>
        <html lang={doc.lang()}>
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <link rel="stylesheet" href={doc.asset("style.css")}>
                {doc.head}
                <script type="module" src={doc.asset("app.js")}></script>
            </head>
            <body>
                <header class="site"><a href={routes::Home}>"Card Market"</a></header>
                <main>{doc.main}</main>
            </body>
        </html>
    }
}
