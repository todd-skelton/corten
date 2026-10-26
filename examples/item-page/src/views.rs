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

/// The page for a failed request (`App::error_page`), in the layout; under `corten dev` it shows
/// the error. The PWA's offline page is `Failure::Offline`.
pub fn error_page(req: &Req, failure: Failure) -> impl Document + '_ {
    let t: Msg = req.t();
    let (title, text) = match failure {
        Failure::NotFound => (t.error_not_found(), t.error_not_found_text()),
        Failure::TimedOut | Failure::Offline => (t.error_unavailable(), t.error_unavailable_text()),
        Failure::WrongMethod
        | Failure::CrossSite
        | Failure::Refused { .. }
        | Failure::Rejected { .. }
        | Failure::Failed { .. }
        | Failure::Panicked { .. } => (t.error_failed(), t.error_failed_text()),
    };
    html! {
        <Layout req={req} title={title} head="">
            <h1>{title}</h1>
            <p>{text}</p>
            if let Some(detail) = failure.detail() {
                <pre>{detail}</pre>
            }
        </Layout>
    }
}

pub fn category(t: Msg, slug: &str) -> impl Render + '_ {
    html! {
        <h1>{t.category_title(slug)}</h1>
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
                <header class="site"><a href={routes::Home}>"Card Market"</a></header>
                <main>{children}</main>
            </body>
        </html>
    }
}
