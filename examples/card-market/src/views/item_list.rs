use corten::prelude::*;

use super::ItemCard;
use crate::{items, source::Shelf, text::Msg};

/// Items as a grid of cards: the ones the source lists, or links to the first ids when it cannot
/// list them (the item API looks items up one at a time).
#[component(css)]
pub fn ItemList<'a>(req: &'a Req, t: Msg, shelf: Shelf<'a>) -> impl Render {
    html! {
        match shelf {
            Shelf::Items(items) => {
                <ul class={List}>
                    for (i, item) in items.iter().enumerate() {
                        // The first row (five cards at the widest the layout gets) is on screen at
                        // once; the rest wait until they are near.
                        <li><ItemCard req={req} t={t} item={item} eager={i < 5} /></li>
                    }
                </ul>
            }
            Shelf::Ids(ids) => {
                <ul class={(List, Ids)}>
                    for n in ids.clone() {
                        let id = format!("item_{n}");
                        <li><a href={items::Item { id: &id }}>{t.home_item(n)}</a></li>
                    }
                </ul>
            }
        }
    }
}
