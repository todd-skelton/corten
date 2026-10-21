//! The item page's markup. Each piece a patch replaces (the alert form, the alert message) is a
//! component the page uses too, so the page and the patch cannot drift.
use corten::prelude::*;

use crate::{
    model::{AlertInput, ItemView, Listing, Offer, sort_listings},
    routes,
    text::Msg,
};

/// The elements `<listing-sort>` finds. `corten::types` writes them as a TypeScript union for
/// `static/islands/listing-sort.js`, and `html!` refuses them inside any other island.
#[derive(Clone, Copy, Refs)]
#[island("listing-sort")]
pub enum ListingSortRef {
    Toggle,
    Rows,
}

/// The elements `<offer-list>` finds.
#[derive(Clone, Copy, Refs)]
#[island("offer-list")]
pub enum OfferListRef {
    Rows,
    Rest,
    More,
}

/// Offers shown before "Show all"; the rest ship in a <template> for the offer-list island.
const OFFERS_SHOWN: usize = 5;

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
        let shown = item.offers.len().min(OFFERS_SHOWN);
        let (offers, rest) = item.offers.split_at(shown);
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
                <section class="listings">
                    <listing-sort data-order="asc">
                        <h2>{t.item_listings()}</h2>
                        <button type="button" class="sort" data-ref={ListingSortRef::Toggle} data-asc={t.item_sort_asc()} data-desc={t.item_sort_desc()}>{t.item_sort_asc()}</button>
                        <table>
                            <thead><tr><th>{t.table_seller()}</th><th>{t.table_condition()}</th><th>{t.table_ships_from()}</th><th>{t.table_qty()}</th><th>{t.table_price()}</th></tr></thead>
                            <tbody data-ref={ListingSortRef::Rows}>for l in listings { <ListingRow l={l} /> }</tbody>
                        </table>
                    </listing-sort>
                </section>
                <section class="offers">
                    <offer-list>
                        <h2>{t.item_offers()}</h2>
                        <table>
                            <thead><tr><th>{t.table_buyer()}</th><th>{t.table_condition()}</th><th>{t.table_qty()}</th><th>{t.table_price()}</th></tr></thead>
                            <tbody data-ref={OfferListRef::Rows}>for o in offers { <OfferRow o={o} /> }</tbody>
                        </table>
                        if !rest.is_empty() {
                            <template data-ref={OfferListRef::Rest}>for o in rest { <OfferRow o={o} /> }</template>
                            <button type="button" class="more" data-ref={OfferListRef::More}>{t.item_show_all_offers(item.offers.len())}</button>
                        }
                    </offer-list>
                </section>
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

#[component]
fn ListingRow(l: &Listing) -> impl Render {
    html! {
        <tr data-price={l.price_value}>
            <td>{l.seller}if let Some(rating) = &l.rating { " · "{rating}" ★ ("{l.reviews}")" }</td>
            <td>{l.condition}</td><td>{l.ships_from}</td><td>{l.quantity}</td><td class="price">{l.price}</td>
        </tr>
    }
}

#[component]
fn OfferRow(o: &Offer) -> impl Render {
    html! { <tr><td>{o.buyer}</td><td>{o.condition}</td><td>{o.quantity}</td><td class="price">{o.price}</td></tr> }
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
