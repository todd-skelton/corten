use corten::prelude::*;

use super::money;
use crate::app::{PriceInput, url};

/// A price cell's id, `price-{item id}`: the price route answers with the cell, which replaces it.
#[derive(Id)]
pub struct PriceId<'a>(pub &'a str);

/// A price that edits in place: typing shows Save, Escape restores the saved price (the input's
/// `value` attribute, its `defaultValue`), and the form posts to the item's price route, which
/// answers with this cell (or the cell with its error).
#[component(css, js)]
pub fn PriceCell<'a>(id: &'a str, cents: Option<u32>, typed: &'a str, errors: &'a Errors) -> impl Render {
    let save = refs();
    let f = PriceInput::FIELDS;
    let saved = cents.map(money);
    html! {
        // Borrowed from `saved`, which the markup owns.
        let shown = saved.as_deref().unwrap_or(typed);
        <form id={PriceId(id)} class={Form} method="post" action={url::SetPrice { id }} data-patch>
            <input
                name={f.price}
                value={shown}
                inputmode="decimal"
                aria-label="Price"
                on:input={js::edited(target, save)}
                on:keydown={js::revert(event, target, save)}
                {..errors.aria(f.price)}
            >
            <button ref={save} hidden={errors.is_empty()}>"Save"</button>
            {errors.message(f.price)}
        </form>
    }
}
