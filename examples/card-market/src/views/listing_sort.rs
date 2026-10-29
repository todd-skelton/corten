use corten::prelude::*;

use super::Price;
use crate::{
    model::{Listing, sort_cents},
    text::{self, Msg},
};

/// The listings, cheapest first, with a button that flips the order in the browser
/// (`listing_sort.js`, the same order `sort_listings` gives). Each row carries the cents it sorts
/// by in `data-cents`: the script only reorders the rows the server drew, which a `data-`
/// attribute is for; a `#[component(view)]` is for rows a script writes or makes (the scripts
/// guide, Views), and would add the views module to the page.
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
                <tbody ref={rows}>for &l in listings { <ListingRow t={t} l={l} /> }</tbody>
            </table>
        </div>
    }
}

#[component]
fn ListingRow(t: Msg, l: &Listing) -> impl Render {
    html! {
        <tr data-cents={sort_cents(l)}>
            <td>
                {l.seller}
                match (l.stars, &l.rating) {
                    (Some(stars), _) => { " · "{t.listing_rating(t.number(stars), l.reviews)} },
                    (None, Some(rating)) => { " · "{t.listing_rating(rating, l.reviews)} },
                    (None, None) => {},
                }
            </td>
            <td>{text::condition(t, l.grade, &l.condition)}</td>
            <td>{l.ships_from}</td>
            <td>{l.quantity}</td>
            <Price t={t} cents={l.cents} sent={&l.price} />
        </tr>
    }
}
