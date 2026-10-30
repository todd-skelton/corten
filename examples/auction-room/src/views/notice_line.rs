use corten::{i18n::Minor, prelude::*};

use crate::{
    app::url,
    room::{News, Notice},
    text::{Msg, USD},
};

/// The notice line's id: the bidder's notices stream replaces it.
#[derive(Id)]
pub struct NoticeId;

/// A bidder's latest news, for them alone: someone outbid them, or they won. Empty (and hidden)
/// until there is some. The `status` role is on the `<corten-live>` around it, which stays while
/// this is replaced, so a screen reader reads each new one.
#[component(css)]
pub fn NoticeLine<'a>(t: Msg, notice: &'a Notice) -> impl Patch {
    html! {
        <p id={NoticeId} class={Notice}>
            match notice.news {
                News::None => {}
                News::Outbid { cents } => {
                    "You were outbid on "
                    <a href={url::Lot { id: &notice.lot }}>{&notice.card}</a>
                    ": it is at "
                    {t.money(Minor(cents), USD)}
                    "."
                }
                News::Won { cents } => {
                    "You won "
                    <a href={url::Lot { id: &notice.lot }}>{&notice.card}</a>
                    " for "
                    {t.money(Minor(cents), USD)}
                    "."
                }
            }
        </p>
    }
}
