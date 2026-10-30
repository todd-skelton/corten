//! The builder's views. Each component's script and stylesheet are next to it in `views/`.
use corten::{Component, prelude::*};

use crate::{
    app::{Builder, Deck, DeckQuery, Sort},
    cards::{Card, TOP_COST},
    text::Msg,
};

corten::assets::embed! {
    /// The files in `static/`, compiled into the program: `Static::STYLE_CSS` is `static/style.css`.
    pub struct Static = "static";
}

mod collection_row;
mod deck_builder;
mod deck_line;
mod save_note;

pub use collection_row::CollectionRow;
pub use deck_builder::{DeckBuilder, DeckFormId};
pub use deck_line::DeckLine;
pub use save_note::{Answer, DeckAnswered, SaveNote};

/// What the app lists in `App::components`: the scripts, the stylesheet and the rows' views.
pub const COMPONENTS: &[Component] = &[DeckBuilder, CollectionRow, DeckLine, SaveNote];

/// The document around the page.
#[component]
pub fn Layout(req: &Req, title: &str, children: impl Render) -> impl Document {
    html! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <link rel="stylesheet" href={req.asset(Static::STYLE_CSS)}>
                <link rel="icon" type="image/svg+xml" href={req.asset(Static::FAVICON_SVG)}>
                <title>{title}</title>
                <Corten />
            </head>
            <body>
                <header><strong>"Deck builder"</strong><span>"Build a deck of up to 40 cards from your collection."</span></header>
                <main>{children}</main>
            </body>
        </html>
    }
}

/// A card in the deck, with its copies.
pub struct Line<'a> {
    pub card: &'a Card,
    pub count: u32,
}

/// A card of the collection as the grid shows it: whether the filter hides it, and how many
/// copies the deck has.
pub struct Shown<'a> {
    pub card: &'a Card,
    pub hidden: bool,
    pub in_deck: u32,
}

/// The screen: the collection, in the order and with the filter the address asks for, and the
/// saved deck, by cost then name.
pub fn screen<'a>(
    t: Msg,
    builder: &'a Builder,
    saved: &'a Deck,
    version: u64,
    view: &'a DeckQuery,
    answer: &'a Answer,
) -> impl Render + 'a {
    let query = view.q.as_deref().unwrap_or("").trim().to_lowercase();
    let mut cards: Vec<Shown> = builder
        .cards()
        .iter()
        .map(|card| Shown {
            card,
            hidden: !query.is_empty() && !matches(card, &query),
            in_deck: saved.get(&card.id).copied().unwrap_or(0),
        })
        .collect();
    match view.sort {
        Some(Sort::Name) => cards.sort_by_cached_key(|s| s.card.name.to_lowercase()),
        Some(Sort::Cost) => cards.sort_by_cached_key(|s| (s.card.cost, s.card.name.to_lowercase())),
        Some(Sort::Owned) => cards.sort_by_cached_key(|s| (std::cmp::Reverse(s.card.owned), s.card.name.to_lowercase())),
        None => {}
    }
    let mut lines: Vec<Line> =
        saved.iter().filter_map(|(id, &count)| builder.card(id).map(|card| Line { card, count })).collect();
    lines.sort_by_cached_key(|l| (l.card.cost, l.card.name.to_lowercase()));
    html! { <DeckBuilder t={t} cards={cards} lines={lines} version={version} view={view} answer={answer} /> }
}

/// Whether the filter's words find `card`: its name, set or kind, as the page's script filters.
fn matches(card: &Card, query: &str) -> bool {
    format!("{} {} {}", card.name, card.set, card.kind.as_str()).to_lowercase().contains(query)
}

/// The deck as text, a line per card (`3 Gravel Sprite`), as other deck tools read it.
pub fn deck_list(lines: &[Line]) -> String {
    lines.iter().map(|l| format!("{} {}", l.count, l.card.name)).collect::<Vec<_>>().join("\n")
}

/// The cost a card counts at on the curve: its own, or the top one for anything above it.
pub fn curve_cost(cost: u32) -> u32 {
    cost.min(TOP_COST)
}