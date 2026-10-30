use std::time::SystemTime;

use corten::prelude::*;

use super::{BidForm, LotStanding};
use crate::{
    app::{Bidder, url},
    room::Live,
    text::Msg,
};

/// A lot's page: the card, its standing kept current by its live stream, and the bid form for
/// someone signed in while it is open.
#[component(css)]
pub fn LotPage<'a>(
    t: Msg,
    live: &'a Live,
    now: SystemTime,
    viewer: Option<&'a Bidder>,
    typed: &'a str,
    errors: &'a Errors,
) -> impl Render {
    let standing = &live.standing;
    let lot = &standing.lot;
    let open = !standing.closed(now);
    html! {
        <p class={Back}><a href={url::Home}>"All lots"</a></p>
        <div class={Columns}>
            <section class={Panel}>
                <h1>{&lot.card}</h1>
                <p class={Meta}>{&lot.set}" · "{&lot.condition}</p>
                <corten-live src={url::LotLive { id: &lot.id }} since={live.version}>
                    <LotStanding t={t} standing={standing} now={now} you={viewer.map(|b| b.name.as_str())} />
                </corten-live>
                <p class={Meta}><a href={url::History { id: &lot.id }} download>"Download every bid (CSV)"</a></p>
            </section>
            <aside class={Panel}>
                match (open, viewer.map(|b| b.house)) {
                    (false, _) => <p>"Bidding on this lot has closed."</p>,
                    (true, Some(true)) => <p>"The house does not bid. End this lot from the house's page."</p>,
                    (true, Some(false)) => <BidForm lot={&lot.id} typed={typed} errors={errors} />,
                    (true, None) => <p><a href={url::SignIn}>"Sign in"</a>" to bid."</p>,
                }
            </aside>
        </div>
    }
}
