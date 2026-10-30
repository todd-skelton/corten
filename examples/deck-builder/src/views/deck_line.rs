use corten::prelude::*;

use super::deck_builder::MoveHelpId;
use crate::app::DeckInput;

/// A card in the deck and its copies. `view` makes `DeckLine` in `corten/views`: the builder's
/// script makes and orders these from the deck it shows (`rows(lines, DeckLine, template).replace`),
/// filled from the collection row of the same card, so every hole is data a script has, and the
/// key, the card's id, is a hole too (the hidden `card` input).
///
/// The count is a number input named `count` beside the hidden `card`, so the deck's form posts
/// each line as `card` and `count`: without JavaScript a player types a count and saves, and a
/// cleared count takes the card out. `lift` is the grip a pointer drags and a keyboard picks up,
/// to take the card out of the deck; `copies` lets the builder's one change handler on the list
/// get the input that changed.
#[component(view)]
pub fn DeckLine<'a>(
    id: &'a str,
    name: &'a str,
    kind: &'a str,
    cost: u32,
    count: u32,
    owned: u32,
    lift: Ref<el::HTMLButtonElement>,
    copies: Ref<el::HTMLInputElement>,
) -> impl Render {
    let f = DeckInput::FIELDS;
    html! {
        <li key={id} data-kind={kind}>
            <input type="hidden" name={f.card} value={id}>
            <button ref={lift} type="button" draggable="true" aria-label={name} aria-describedby={MoveHelpId}>"⠿"</button>
            <span>{cost}</span>
            <span>{name}</span>
            <small>{kind}</small>
            <input ref={copies} type="number" name={f.count} value={count} min="0" inputmode="numeric" aria-label={name}>
            <small>"of "<span>{owned}</span></small>
        </li>
    }
}
