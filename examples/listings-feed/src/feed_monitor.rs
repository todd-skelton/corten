//! The seller's page: the listings feed followed live, with what has arrived. The failover
//! benchmark keeps one open in a real browser through the load balancer while it kills instances.
use corten::prelude::*;

use crate::app::url;

pub fn page(seller: &str) -> impl Document + '_ {
    html! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>"Listings for " {seller}</title>
                <Corten />
            </head>
            <body>
                <main>
                    <h1>"Listings for " {seller}</h1>
                    <p>
                        "This page follows the seller's listings as they change. Stop the instance it "
                        "streams from: it resumes on another one from the same cursor, and misses no change."
                    </p>
                    <FeedMonitor seller={seller} />
                </main>
            </body>
        </html>
    }
}

/// The feed's state as the page sees it: its connection, the listings in the last snapshot, the
/// sequence it has reached, and how many changes and snapshots arrived. A change that skips a
/// sequence counts as missed. The outputs hold the counts, so the script keeps no state.
#[component(js)]
pub fn FeedMonitor<'a>(seller: &'a str) -> impl Render {
    let (status, listings, through, changes, snapshots, missed) = refs();
    let shown = js::Shown { status, listings, through, changes, snapshots, missed };
    html! {
        <dl on:connect={js::follow(shown, url::Listings { seller }, signal)}>
            <dt>"Status"</dt>
            <dd><output ref={status}>"connecting"</output></dd>
            <dt>"Listings"</dt>
            <dd><output ref={listings}>"0"</output></dd>
            <dt>"Through"</dt>
            <dd><output ref={through}>"0"</output></dd>
            <dt>"Changes"</dt>
            <dd><output ref={changes}>"0"</output></dd>
            <dt>"Snapshots"</dt>
            <dd><output ref={snapshots}>"0"</output></dd>
            <dt>"Missed"</dt>
            <dd><output ref={missed}>"0"</output></dd>
        </dl>
    }
}
