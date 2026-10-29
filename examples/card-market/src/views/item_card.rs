use corten::{i18n::Minor, prelude::*};

use crate::{
    items,
    model::ItemView,
    text::{Msg, USD},
};

/// An item in a list (the home page, a category): its picture, its name, its set and the lowest
/// price, as one link to its page. The picture is a versioned static file, sized so the grid does
/// not move as it loads; past the first row (`eager`) it loads only near the screen; the link's text names the card, so the
/// picture adds nothing for a screen reader (`alt=""`).
#[component(css)]
pub fn ItemCard<'a>(req: &'a Req, t: Msg, item: &'a ItemView, eager: bool) -> impl Render {
    html! {
        <a class={Card} href={items::Item { id: &item.id }}>
            if let Some(art) = item.art {
                <img class={Art} src={req.asset(art)} width="250" height="350" alt="" loading={(!eager).then_some("lazy")}>
            }
            <strong>{item.title}</strong>
            if let Some(subtitle) = &item.subtitle { <span class={Set}>{subtitle}</span> }
            match item.summary.as_ref().map(|s| (s.lowest_cents, s.lowest.as_deref())) {
                Some((Some(cents), _)) => <span class={From}>{t.card_from(t.money(Minor(cents), USD))}</span>,
                Some((None, Some(sent))) => <span class={From}>{t.card_from(sent)}</span>,
                _ => <span class={From}>{t.item_no_listings()}</span>,
            }
        </a>
    }
}
