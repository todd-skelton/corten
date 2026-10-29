use corten::prelude::*;

use crate::text::Msg;

/// The item count's id: archiving patches it.
#[derive(Id)]
pub struct ItemCountId;

/// How many items the desk holds, at the end of the grid's toolbar. The server writes it, and an
/// archive's answer replaces it, so it counts what the server kept; the grid's filter says how
/// many of them it shows beside it.
#[component(css)]
pub fn ItemCount(t: Msg, count: usize) -> impl Patch {
    html! {
        <p id={ItemCountId} class={Count}>
            if count == 1 { {t.grid_items_one(t.number(count))} } else { {t.grid_items_other(t.number(count))} }
        </p>
    }
}
