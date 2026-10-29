//! The seller's page: who the seller is, the form that moves some market lows, and the listings
//! the feed keeps current. The failover benchmark keeps one open in a real browser through the
//! load balancer while it kills instances.
use corten::prelude::*;

use crate::{
    app::{MoveInput, url},
    listing_table::{Line, ListingTable},
    text::Msg,
};

corten::assets::embed! {
    /// The files in `static/`, compiled into the program: `Static::STYLE_CSS` is `static/style.css`.
    pub struct Static = "static";
}

/// The status line's id: moving the market lows answers with it.
#[derive(Id)]
pub struct MoveStatusId;

/// What the last move did.
#[component]
pub fn MoveStatus(text: impl Render) -> impl Patch {
    html! { <p id={MoveStatusId} role="status">{text}</p> }
}

pub fn page<'a>(
    req: &'a Req,
    seller: &'a str,
    instance: &'a str,
    lines: &'a [Line<'a>],
    total: usize,
    t: Msg,
) -> impl Document + 'a {
    html! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <link rel="stylesheet" href={req.asset(Static::STYLE_CSS)}>
                <title>"Listings for "{seller}</title>
                <Corten />
            </head>
            <body>
                <main>
                    <header>
                        <div>
                            <h1>"Listings for "{seller}</h1>
                            <p>
                                "Served by instance "<strong>{instance}</strong>". Every instance follows the same feed: "
                                "stop this one and the page resumes on another from the same cursor, missing no change."
                            </p>
                        </div>
                        // A write commits to Postgres with the feed's outbox row; the rows change
                        // when the feed brings it, here and on every instance. Its submit key makes
                        // a resend (a lost answer, a retry) commit once.
                        <form method="post" action={url::Move { seller }} data-patch>
                            {SubmitKey::field(MoveInput::FIELDS.submit)}
                            <button>"Move five market lows"</button>
                        </form>
                    </header>
                    <MoveStatus text="" />
                    <ListingTable seller={seller} lines={lines} total={total} t={t} />
                </main>
            </body>
        </html>
    }
}
