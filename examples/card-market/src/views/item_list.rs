use corten::prelude::*;

use super::ItemCard;
use crate::{routes, source::Shelf, text::Msg};

/// Items as a grid of cards: the ones the source lists, or links to the first ids when it cannot
/// list them (the item API looks items up one at a time).
#[component(css)]
pub fn ItemList<'a>(t: Msg, shelf: Shelf<'a>) -> impl Render {
    html! {
        match shelf {
            Shelf::Items(items) => {
                <ul class={List}>for item in items.iter() { <li><ItemCard t={t} item={item} /></li> }</ul>
            }
            Shelf::Ids(ids) => {
                <ul class={(List, Ids)}>
                    for n in ids.clone() {
                        let id = format!("item_{n}");
                        <li><a href={routes::Item { id: &id }}>{t.home_item(n)}</a></li>
                    }
                </ul>
            }
        }
    }
}
