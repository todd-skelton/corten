use std::time::SystemTime;

use corten::{
    i18n::{Minor, RelativeStyle},
    prelude::*,
};
use corten_ui::RelativeTime;

use super::BidCount;
use crate::{
    store::Standing,
    text::{Msg, USD},
};

/// A lot's standing's id: its live stream, and a bid's answer, replace it.
#[derive(Id)]
pub struct StandingId;

/// A lot as it stands, for `you` (who is looking, if signed in): the price, the least the next
/// bid may be, the time left, whether you lead, and the latest bids.
#[component(css)]
pub fn LotStanding<'a>(t: Msg, standing: &'a Standing, now: SystemTime, you: Option<&'a str>) -> impl Patch {
    let open = !standing.closed(now);
    let top = standing.top();
    let leads = top.is_some_and(|bid| Some(bid.bidder.as_str()) == you);
    html! {
        <section id={StandingId} class={Box}>
            <p class={Price}>{t.money(Minor(top.map_or(standing.lot.opening_cents, |bid| bid.cents)), USD)}</p>
            <p class={Muted}>
                match (open, top) {
                    (true, None) => { "Opening price · no bids yet" }
                    (true, Some(_)) => { <BidCount t={t} count={standing.count} /> }
                    (false, Some(top)) => {
                        "Sold to "{&top.bidder}" · "
                        <BidCount t={t} count={standing.count} />
                    }
                    (false, None) => { "Closed without a bid" }
                }
            </p>
            if open {
                <p>
                    "Next bid "
                    <strong>{t.money(Minor(standing.minimum()), USD)}</strong>
                    " or more · ends "
                    <RelativeTime when={t.relative(standing.lot.ends_at, now, RelativeStyle::Long)} />
                </p>
            }
            if leads {
                <p class={Lead}>if open { "You lead." } else { "You won this lot." }</p>
            }
            if !standing.recent.is_empty() {
                <ol class={Bids} aria-label="Latest bids">
                    for bid in &standing.recent {
                        <li>
                            <span>{&bid.bidder}</span>
                            <span class={Amount}>{t.money(Minor(bid.cents), USD)}</span>
                            <RelativeTime when={t.relative(bid.at, now, RelativeStyle::Short)} />
                        </li>
                    }
                </ol>
            }
        </section>
    }
}
