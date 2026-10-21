//! The item page's markup. Each piece a patch replaces (the alert form, the alert message) is a
//! component the page uses too, so the page and the patch cannot drift. The two islands live in
//! `views/`, each Rust file next to its script.
use corten::{Island, prelude::*};

use crate::{
    model::{AlertInput, ItemView, Listing, sort_listings},
    routes,
    text::Msg,
};

mod listing_sort;
mod offer_list;

pub use listing_sort::ListingSort;
pub use offer_list::OfferList;

/// Every island the app renders, for `App::islands` and the generated types.
pub const ISLANDS: &[Island] = &[ListingSort::ISLAND, OfferList::ISLAND];

/// The item page borrows the item: rendering copies no strings out of it.
pub struct ItemPage<'a> {
    pub t: Msg,
    pub item: &'a ItemView,
    pub listings: Vec<&'a Listing>,
    pub message: String,
    pub form: AlertFormView<'a>,
}

impl<'a> ItemPage<'a> {
    pub fn new(t: Msg, item: &'a ItemView, message: String) -> Self {
        let listings = sort_listings(&item.listings, false);
        let form = AlertFormView { id: &item.id, title: &item.title, ..AlertFormView::default() };
        ItemPage { t, item, listings, message, form }
    }

    pub fn meta(&self) -> Meta {
        Meta::new(format!("{} | Card Market", self.item.title))
            .description(&self.item.description)
            .canonical(&routes::Item { id: &self.item.id }.href())
    }

    pub fn view(self) -> impl Render + 'a {
        let ItemPage { t, item, listings, message, form } = self;
        html! {
            <article class="item">
                <nav class="crumbs">for c in &item.categories { <a href={routes::Category { slug: &c.slug }}>{c.name}</a> }</nav>
                <h1>{item.title}</h1>
                if let Some(subtitle) = &item.subtitle { <p class="subtitle">{subtitle}</p> }
                <ul class="badges">for badge in &item.badges { <li>{badge}</li> }</ul>
                <div class="gallery">
                    for (i, url) in item.images.iter().enumerate() {
                        <div class="img" role="img" aria-label={t.item_image(&item.title, i + 1)} data-src={url}></div>
                    }
                </div>
                <section class="summary">
                    match &item.summary {
                        Some(s) => {
                            <span><strong>{s.lowest.as_deref().unwrap_or("—")}</strong>" "{t.item_lowest()}</span>
                            <span>{t.item_listing_count(s.count)}</span>
                            <span>{t.item_available(s.quantity)}</span>
                        }
                        None => <span>{t.item_no_listings()}</span>,
                    }
                </section>
                <section class="listings"><ListingSort t={t} listings={&listings} /></section>
                <section class="offers"><OfferList t={t} offers={&item.offers} /></section>
                <section class="alert">
                    <h2>{t.alert_title()}</h2>
                    <AlertForm t={t} form={form} />
                    <AlertMessage message={&message} />
                </section>
                <section class="details">
                    <h2>{t.item_details()}</h2>
                    <p>{item.description}</p>
                    <dl>for f in &item.fields { <dt>{f.name}</dt><dd>{f.value}</dd> }</dl>
                    <ul class="tags">for tag in &item.tags { <li>{tag}</li> }</ul>
                </section>
                if !item.contents.is_empty() {
                    <section class="contents">
                        <h2>{t.item_contents()}</h2>
                        <ul>for c in &item.contents { <li><a href={routes::Item { id: &c.item_id }}>{c.title}</a>" × "{c.quantity}</li> }</ul>
                    </section>
                }
                if !item.included_in.is_empty() {
                    <section class="included">
                        <h2>{t.item_included_in()}</h2>
                        <ul>for c in &item.included_in { <li><a href={routes::Item { id: &c.item_id }}>{c.title}</a>" × "{c.quantity}</li> }</ul>
                    </section>
                }
            </article>
        }
    }
}

/// The price-alert form's values and errors. The item page shows the form; a submit that fails
/// validation patches it on its own. Field names come from `AlertInput::FIELDS`.
#[derive(Clone, Default)]
pub struct AlertFormView<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub email: &'a str,
    pub target_price: &'a str,
    pub errors: Errors,
}

#[component]
pub fn AlertForm<'a>(t: Msg, form: AlertFormView<'a>) -> impl Render {
    let f = AlertInput::FIELDS;
    html! {
        <div class="alert-form" id="alert-form">
            <form method="post" action={routes::Item { id: form.id }} data-patch>
                <input type="hidden" name={f.title} value={form.title}>
                <label>{t.alert_email()}<input type="email" name={f.email} value={form.email} required {..form.errors.aria(f.email)}></label>
                {form.errors.message(f.email)}
                <label>{t.alert_below()}<input type="text" name={f.target_price} value={form.target_price} inputmode="decimal" required {..form.errors.aria(f.target_price)}></label>
                {form.errors.message(f.target_price)}
                <button type="submit">{t.alert_submit()}</button>
            </form>
        </div>
    }
}

/// The price-alert result, inside the item page and on its own as a patch.
#[component]
pub fn AlertMessage(message: &str) -> impl Render {
    html! { <p class="alert-message" id="alert-message" aria-live="polite">{message}</p> }
}

pub fn home(t: Msg) -> impl Render {
    html! {
        <h1>{t.home_title()}</h1>
        <ul class="home-list">
            for n in 1..=50 {
                let id = format!("item_{n}");
                <li><a href={routes::Item { id: &id }}>"Card Name "{n}</a></li>
            }
        </ul>
    }
}

pub fn category(t: Msg, slug: &str) -> impl Render + '_ {
    html! {
        <h1>{t.category_title(slug)}</h1>
        <p><a href={routes::Home}>{t.category_back()}</a></p>
    }
}

/// The document around every page.
pub fn shell(doc: Document) -> impl Render {
    html! {
        <!DOCTYPE html>
        <html lang={doc.lang()}>
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                {doc.head}
                <link rel="stylesheet" href={doc.asset("style.css")}>
                <script type="module" src={doc.asset("app.js")}></script>
            </head>
            <body>
                <header class="site"><a href={routes::Home}>"Card Market"</a></header>
                <main>{doc.main}</main>
            </body>
        </html>
    }
}
