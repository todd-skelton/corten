use corten::prelude::*;

use crate::{model::Listing, text::Msg};

/// The listings, cheapest first, with a button that flips the order in the browser
/// (`listing_sort.js`, the same order `sort_listings` gives).
#[island]
pub fn ListingSort<'a>(t: Msg, listings: &'a [&'a Listing]) -> impl Render {
    html! {
        <h2>{t.item_listings()}</h2>
        <button type="button" class="sort" on:click={Toggle}>
            <span ref={Ascending}>{t.item_sort_asc()}</span>
            <span ref={Descending} hidden>{t.item_sort_desc()}</span>
        </button>
        <table>
            <thead><tr><th>{t.table_seller()}</th><th>{t.table_condition()}</th><th>{t.table_ships_from()}</th><th>{t.table_qty()}</th><th>{t.table_price()}</th></tr></thead>
            <tbody ref={Rows}>for &l in listings { <ListingRow l={l} /> }</tbody>
        </table>
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
