use corten::prelude::*;

use super::Price;
use crate::{
    model::Offer,
    text::{self, Msg},
};

/// Offers shown before "Show all".
const OFFERS_SHOWN: usize = 5;

/// The first offers, with the rest in a `<template>` that "Show all" moves into the table
/// (`offer_list.js`). Without JavaScript the first offers show.
#[component(js)]
pub fn OfferList<'a>(t: Msg, offers: &'a [Offer]) -> impl Render {
    let (rows, rest) = refs();
    let (first, more) = offers.split_at(offers.len().min(OFFERS_SHOWN));
    html! {
        <div>
            <h2>{t.item_offers()}</h2>
            <table>
                <thead>
                    <tr>
                        <th>{t.table_buyer()}</th>
                        <th>{t.table_condition()}</th>
                        <th>{t.table_qty()}</th>
                        <th>{t.table_price()}</th>
                    </tr>
                </thead>
                <tbody ref={rows}>for o in first { <OfferRow t={t} o={o} /> }</tbody>
            </table>
            if !more.is_empty() {
                <template ref={rest}>for o in more { <OfferRow t={t} o={o} /> }</template>
                <button type="button" on:click={js::showAll(rows, rest, target)}>
                    {t.item_show_all_offers(offers.len())}
                </button>
            }
        </div>
    }
}

#[component]
fn OfferRow(t: Msg, o: &Offer) -> impl Render {
    html! {
        <tr>
            <td>{o.buyer}</td>
            <td>{text::condition(t, o.grade, &o.condition)}</td>
            <td>{o.quantity}</td>
            <Price t={t} cents={o.cents} sent={&o.price} />
        </tr>
    }
}
