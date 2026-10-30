//! The auction's views. Each component's stylesheet is next to it in `views/`.
use corten::{Component, axum::http::StatusCode, prelude::*};

use crate::{
    app::{Auction, Bidder, door, house, me, url},
    room::Notice,
    text::Msg,
};

corten::assets::embed! {
    /// The files in `static/`, compiled into the program: `Static::STYLE_CSS` is `static/style.css`.
    pub struct Static = "static";
}

mod bid_form;
mod board;
mod house_desk;
mod lot_page;
mod lot_standing;
mod notice_line;
mod sign_in_form;

pub use bid_form::BidForm;
pub use board::Board;
pub use house_desk::HouseDesk;
pub use lot_page::LotPage;
pub use lot_standing::LotStanding;
pub use notice_line::NoticeLine;
pub use sign_in_form::SignInForm;

/// The components with a stylesheet, for `App::components`.
pub const COMPONENTS: &[Component] = &[BidForm, Board, HouseDesk, LotPage, LotStanding, NoticeLine, SignInForm];

/// Who is signed in, as the layout shows them: their name, whether they are the house, and their
/// latest news, where their live notices start from.
pub struct You {
    pub name: String,
    pub house: bool,
    pub notice: Notice,
}

/// The document around every page. A buyer signed in gets their notices, streamed in under the
/// header from `/me/notices`, which stays open as they move between pages.
#[component]
pub fn Layout<'a>(req: &Req, title: &str, you: Option<&'a You>, head: impl Render, children: impl Render) -> impl Document {
    html! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <link rel="stylesheet" href={req.asset(Static::STYLE_CSS)}>
                <link rel="icon" type="image/svg+xml" href={req.asset(Static::FAVICON_SVG)}>
                <title>{title}" · Card auction"</title>
                {head}
                <Corten />
            </head>
            <body>
                <header>
                    <strong>"Card auction"</strong>
                    <nav>
                        <a {..req.nav(&url::Home)}>"Lots"</a>
                        if you.is_some_and(|you| you.house) {
                            <a {..req.nav(&house::House)}>"House"</a>
                        }
                    </nav>
                    match you {
                        Some(you) => {
                            <form method="post" action={url::SignOut}>
                                <span>{&you.name}</span>
                                <button>"Sign out"</button>
                            </form>
                        }
                        None => <a href={door::SignIn}>"Sign in"</a>,
                    }
                </header>
                // A buyer's own news; the house bids on nothing, so it has none.
                if let Some(you) = you.filter(|you| !you.house) {
                    <corten-live src={me::Notices} since={you.notice.seq} role="status">
                        <NoticeLine t={req.t()} notice={&you.notice} />
                    </corten-live>
                }
                <main>{children}</main>
            </body>
        </html>
    }
}

/// A count of bids, in its plural.
#[component]
pub fn BidCount(t: Msg, count: u64) -> impl Render {
    html! {
        if count == 1 { {t.lot_bids_one(t.number(count))} } else { {t.lot_bids_other(t.number(count))} }
    }
}

/// The page for a failed request (`App::error_page`), in the layout, with who is signed in; under
/// `corten dev` it shows the error.
pub async fn error_page(
    failure: Failure,
    req: Req,
    State(auction): State<Auction>,
    viewer: Option<Caller<Bidder>>,
) -> HtmlResult {
    let you = viewer.map(|Caller(b)| auction.you(&b.name, b.house));
    let (title, text) = match failure {
        Failure::NotFound => ("Not found", "There is no lot or page at this address."),
        Failure::Refused { status: StatusCode::FORBIDDEN } => {
            ("Not for you", "The house's desk is the house's alone, and the house does not bid.")
        }
        Failure::Refused { status: StatusCode::TOO_MANY_REQUESTS } => {
            ("Too many tries", "You sent many in the last minute. Wait a moment, then try again.")
        }
        Failure::Unidentified { .. } | Failure::TimedOut | Failure::Offline => {
            ("Unavailable", "The auction cannot answer right now. Try again in a moment.")
        }
        _ => ("Something went wrong", "Try again, or go back to the lots."),
    };
    req.page(html! {
        <Layout req={&req} title={title} you={you.as_ref()} head="">
            <h1>{title}</h1>
            <p>{text}</p>
            <p><a href={url::Home}>"Back to the lots"</a></p>
            if let Some(detail) = failure.detail() {
                <pre>{detail}</pre>
            }
        </Layout>
    })
}
