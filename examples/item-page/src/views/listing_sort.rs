use corten::prelude::*;

use super::Price;
use crate::{model::Listing, text::Msg};

/// The listings, cheapest first, with a button that flips the order in the browser
/// (`listing_sort.js`, the same order `sort_listings` gives).
#[component(js)]
pub fn ListingSort<'a>(t: Msg, listings: &'a [&'a Listing]) -> impl Render {
    let (rows, ascending, descending) = refs();
    html! {
        <div>
            <h2>{t.item_listings()}</h2>
            <button type="button" on:click={js::toggle(rows, ascending, descending)}>
                <span ref={ascending}>{t.item_sort_asc()}</span>
                <span ref={descending} hidden>{t.item_sort_desc()}</span>
            </button>
            <table>
                <thead>
                    <tr>
                        <th>{t.table_seller()}</th>
                        <th>{t.table_condition()}</th>
                        <th>{t.table_ships_from()}</th>
                        <th>{t.table_qty()}</th>
                        <th>{t.table_price()}</th>
                    </tr>
                </thead>
                <tbody ref={rows}>for &l in listings { <ListingRow l={l} /> }</tbody>
            </table>
        </div>
    }
}

#[component]
fn ListingRow(l: &Listing) -> impl Render {
    html! {
        <tr data-price={l.price_value}>
            <td>
                {l.seller}
                if let Some(rating) = &l.rating { " · "{rating}" ★ ("{l.reviews}")" }
            </td>
            <td>{l.condition}</td>
            <td>{l.ships_from}</td>
            <td>{l.quantity}</td>
            <Price amount={&l.price} />
        </tr>
    }
}
