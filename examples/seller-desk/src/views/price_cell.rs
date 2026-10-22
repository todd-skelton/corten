use corten::prelude::*;

use super::money;
use crate::app::{PriceInput, url};

/// A price that edits in place: typing shows Save, Escape restores the saved price, and the form
/// posts to the item's price route, which answers with this cell (or the cell with its error).
#[component(css, js)]
pub fn PriceCell<'a>(id: &'a str, cents: Option<u32>, typed: &'a str, errors: &'a Errors) -> impl Render {
    let (input, save) = refs();
    let f = PriceInput::FIELDS;
    let saved = cents.map(money).unwrap_or_default();
    html! {
        <form id={("price-", id)} class={Form} method="post" action={url::SetPrice { id }} data-patch>
            <input
                ref={input}
                name={f.price}
                value={if cents.is_some() { saved.as_str() } else { typed }}
                inputmode="decimal"
                aria-label="Price"
                on:input={js::edited(target, save, &saved)}
                on:keydown={js::revert(event, target, save, &saved)}
                {..errors.aria(f.price)}
            >
            <button ref={save} hidden={errors.is_empty()}>"Save"</button>
            {errors.message(f.price)}
        </form>
    }
}
