use corten::prelude::*;

use crate::{model::ItemView, routes, text::Msg};

/// An item in a list (the home page, a category): its name, its set and the lowest price, as one
/// link to its page.
#[component(css)]
pub fn ItemCard<'a>(t: Msg, item: &'a ItemView) -> impl Render {
    html! {
        <a class={Card} href={routes::Item { id: &item.id }}>
            <strong>{item.title}</strong>
            if let Some(subtitle) = &item.subtitle { <span class={Set}>{subtitle}</span> }
            match item.summary.as_ref().and_then(|s| s.lowest.as_deref()) {
                Some(lowest) => <span class={From}>{t.card_from(lowest)}</span>,
                None => <span class={From}>{t.item_no_listings()}</span>,
            }
        </a>
    }
}
