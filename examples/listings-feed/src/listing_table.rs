//! The seller's listings, kept current by the feed, and the feed's state as the page sees it.
use corten::prelude::*;

use crate::{
    app::url,
    listing::Listing,
    listing_row::ListingRow,
    text::{Msg, USD},
};

/// The most listings the page draws: the first by id. The feed carries them all, so the counts
/// are the whole seller's.
pub const SHOWN: usize = 500;

/// A listing as its row shows it: its prices written in the page's locale.
pub struct Line<'a> {
    pub listing: &'a Listing,
    pub price: String,
    pub market: String,
}

impl<'a> Line<'a> {
    pub fn new(t: Msg, listing: &'a Listing) -> Self {
        let money = |cents: i64| t.money(corten::i18n::Minor(cents), USD).to_string();
        Line { listing, price: money(listing.price_cents), market: money(listing.market_cents) }
    }
}

/// The feed's state (its connection, how many listings its last snapshot had, the sequence it has
/// reached, how many changes and snapshots arrived) and the listings, which one subscription keeps
/// current: each change redraws the rows it names that are shown. `subscribe` loads a new
/// snapshot for a gap, so more than one snapshot means the page caught up that way. The outputs
/// are named for what reads them (the failover benchmark), and hold the counts, so the script
/// keeps no state.
#[component(css, js)]
pub fn ListingTable<'a>(seller: &'a str, lines: &'a [Line<'a>], total: usize, t: Msg) -> impl Render {
    let (status, listings, through, changes, snapshots, body) = refs();
    let shown = js::Shown { status, listings, through, changes, snapshots };
    html! {
        <section class={Listings} on:connect={js::follow(shown, body, t.js().money(USD), url::Listings { seller }, signal)}>
            <dl class={Feed}>
                <div>
                    <dt>"Feed"</dt>
                    <dd><output name="status" ref={status}>"syncing"</output></dd>
                </div>
                <div>
                    <dt>"Listings"</dt>
                    <dd><output name="listings" ref={listings}>{total}</output></dd>
                </div>
                <div>
                    <dt>"Through change"</dt>
                    <dd><output name="through" ref={through}>"0"</output></dd>
                </div>
                <div>
                    <dt>"Changes"</dt>
                    <dd><output name="changes" ref={changes}>"0"</output></dd>
                </div>
                <div>
                    <dt>"Snapshots"</dt>
                    <dd><output name="snapshots" ref={snapshots}>"0"</output></dd>
                </div>
            </dl>
            <p class={Count}>{t.listings_shown(t.number(lines.len()), t.number(total))}</p>
            <div class={Scroll}>
                <table class={Table}>
                    <thead>
                        <tr>
                            <th>"Listing"</th>
                            <th>"Category"</th>
                            <th>"Condition"</th>
                            <th>"Qty"</th>
                            <th>"Price"</th>
                            <th>"Market low"</th>
                        </tr>
                    </thead>
                    <tbody ref={body}>
                        for line in lines {
                            <ListingRow
                                id={&line.listing.id}
                                title={&line.listing.title}
                                category={&line.listing.category}
                                condition={&line.listing.condition}
                                price={&line.price}
                                market={&line.market}
                                qty={line.listing.quantity.max(0) as u32}
                                paused={line.listing.status == "paused"}
                                above={line.listing.price_cents > line.listing.market_cents}
                            />
                        }
                    </tbody>
                </table>
            </div>
        </section>
    }
}
