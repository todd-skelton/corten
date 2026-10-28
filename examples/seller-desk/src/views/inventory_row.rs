use corten::prelude::*;

use super::{ArchiveFormId, PriceCell};
use crate::app::ArchiveInput;

/// A row's id, `row-{item id}`: the archive patch removes rows by it.
#[derive(Id)]
pub struct RowId<'a>(pub &'a str);

/// One item. `view` makes `InventoryRow` in `corten/views`, whose holes the grid's script writes
/// by name: `selected` (the checkbox and the row's mark), `name` and `set` (the filter and the
/// sort), `qty` (the live levels).
/// Its key, the item's id, is also the price cell's and the row id the archive patch removes, so
/// the server makes the rows and scripts find them by key (`rows(body, InventoryRow).get(id)`).
/// `select` is the grid's ref: its one click handler on the table body gets the checkbox.
#[component(view)]
pub fn InventoryRow<'a>(
    id: &'a str,
    name: &'a str,
    set: &'a str,
    condition: &'a str,
    qty: u32,
    price_cents: u32,
    selected: bool,
    select: Ref<el::HTMLInputElement>,
) -> impl Render {
    html! {
        <tr key={id} id={RowId(id)} data-selected={selected}>
            <td>
                <input
                    ref={select}
                    type="checkbox"
                    name={ArchiveInput::FIELDS.ids}
                    value={id}
                    form={ArchiveFormId}
                    checked={selected}
                    aria-label={name}
                >
            </td>
            <td>{name}</td>
            <td>{set}</td>
            <td>{condition}</td>
            <td>{qty}</td>
            <td><PriceCell id={id} cents={Some(price_cents)} typed="" errors={&Errors::new()} /></td>
        </tr>
    }
}
