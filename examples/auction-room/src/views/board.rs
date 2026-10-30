use std::{sync::Arc, time::SystemTime};

use corten::{
    i18n::{Minor, RelativeStyle},
    prelude::*,
};
use corten_ui::RelativeTime;

use super::BidCount;
use crate::{
    app::url,
    store::Standing,
    text::{Msg, USD},
};

/// The lobby's list's id: its live stream replaces it.
#[derive(Id)]
pub struct BoardId;

/// Every lot: its price, its bids and the time left while it is open; who won once it closed.
#[component(css)]
pub fn Board<'a>(t: Msg, lots: &'a [Arc<Standing>], now: SystemTime) -> impl Patch {
    html! {
        <ul id={BoardId} class={Lots}>
            for standing in lots {
                let open = !standing.ended(now);
                let price = standing.top().map_or(standing.lot.opening_cents, |bid| bid.cents);
                <li class={(Item, (!open).then_some(Closed))}>
                    <a class={Name} href={url::Lot { id: &standing.lot.id }}>{&standing.lot.card}</a>
                    <span class={Meta}>{&standing.lot.set}" · "{&standing.lot.condition}</span>
                    <span class={Price}>{t.money(Minor(price), USD)}</span>
                    <span class={Meta}>
                        match (open, standing.top()) {
                            (true, _) => {
                                <BidCount t={t} count={standing.count} />
                                " · ends "
                                <RelativeTime when={t.relative(standing.lot.ends_at, now, RelativeStyle::Short)} />
                            }
                            (false, Some(top)) => { "Sold to "{&top.bidder} }
                            (false, None) => { "Closed without a bid" }
                        }
                    </span>
                </li>
            }
        </ul>
    }
}
