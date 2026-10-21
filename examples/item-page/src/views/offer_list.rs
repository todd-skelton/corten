use corten::prelude::*;

use crate::{model::Offer, text::Msg};

/// Offers shown before "Show all".
const OFFERS_SHOWN: usize = 5;

/// The first offers, with the rest in a `<template>` that "Show all" moves into the table
/// (`offer_list.js`). Without JavaScript the first offers show.
#[island]
pub fn OfferList<'a>(t: Msg, offers: &'a [Offer]) -> impl Render {
    let (first, rest) = offers.split_at(offers.len().min(OFFERS_SHOWN));
    html! {
        <h2>{t.item_offers()}</h2>
        <table>
            <thead><tr><th>{t.table_buyer()}</th><th>{t.table_condition()}</th><th>{t.table_qty()}</th><th>{t.table_price()}</th></tr></thead>
            <tbody ref={Rows}>for o in first { <OfferRow o={o} /> }</tbody>
        </table>
        if !rest.is_empty() {
            <template ref={Rest}>for o in rest { <OfferRow o={o} /> }</template>
            <button type="button" class="more" on:click={ShowAll}>{t.item_show_all_offers(offers.len())}</button>
        }
    }
}

#[component]
fn OfferRow(o: &Offer) -> impl Render {
    html! { <tr><td>{o.buyer}</td><td>{o.condition}</td><td>{o.quantity}</td><td class="price">{o.price}</td></tr> }
}
