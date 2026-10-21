use corten::prelude::*;

use super::{AlertForm, AlertFormView, AlertMessage, ListingSort, OfferList};
use crate::{
    model::{ItemView, Listing, sort_listings},
    routes,
    text::Msg,
    theme::{Direction, Responsive, Space, direction, gap},
};

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

    pub fn view(self) -> ItemDetail<'a> {
        ItemDetail { page: self }
    }
}

/// The item page's markup. Each piece a patch replaces (the alert form, the alert message) is a
/// component the page uses too, so the page and the patch cannot drift.
#[component(css)]
pub fn ItemDetail<'a>(page: ItemPage<'a>) -> impl Render {
    let (t, item, message) = (page.t, page.item, page.message.as_str());
    // A column on phones, a row from the small breakpoint up.
    let summary = (Panel, Summary, gap(Space::_5), direction(Responsive::new(Direction::Column).sm(Direction::Row)));
    html! {
        <article>
            <nav class={Crumbs}>for c in &item.categories { <a href={routes::Category { slug: &c.slug }}>{c.name}</a> }</nav>
            <h1>{item.title}</h1>
            if let Some(subtitle) = &item.subtitle { <p class={Subtitle}>{subtitle}</p> }
            <ul class={Chips}>for badge in &item.badges { <li>{badge}</li> }</ul>
            <div class={Gallery}>
                for (i, url) in item.images.iter().enumerate() {
                    <div class={Image} role="img" aria-label={t.item_image(&item.title, i + 1)} data-src={url}></div>
                }
            </div>
            <section class={summary}>
                match &item.summary {
                    Some(s) => {
                        <span><strong>{s.lowest.as_deref().unwrap_or("—")}</strong>" "{t.item_lowest()}</span>
                        <span>{t.item_listing_count(s.count)}</span>
                        <span>{t.item_available(s.quantity)}</span>
                    }
                    None => <span>{t.item_no_listings()}</span>,
                }
            </section>
            <section class={Panel}><ListingSort t={t} listings={&page.listings} /></section>
            <section class={Panel}><OfferList t={t} offers={&item.offers} /></section>
            <section class={Panel}>
                <h2>{t.alert_title()}</h2>
                <AlertForm t={t} form={page.form.clone()} />
                <AlertMessage message={message} />
            </section>
            <section class={Panel}>
                <h2>{t.item_details()}</h2>
                <p>{item.description}</p>
                <dl>for f in &item.fields { <dt>{f.name}</dt><dd>{f.value}</dd> }</dl>
                <ul class={Chips}>for tag in &item.tags { <li>{tag}</li> }</ul>
            </section>
            if !item.contents.is_empty() {
                <section class={Panel}>
                    <h2>{t.item_contents()}</h2>
                    <ul>for c in &item.contents { <li><a href={routes::Item { id: &c.item_id }}>{c.title}</a>" × "{c.quantity}</li> }</ul>
                </section>
            }
            if !item.included_in.is_empty() {
                <section class={Panel}>
                    <h2>{t.item_included_in()}</h2>
                    <ul>for c in &item.included_in { <li><a href={routes::Item { id: &c.item_id }}>{c.title}</a>" × "{c.quantity}</li> }</ul>
                </section>
            }
        </article>
    }
}
