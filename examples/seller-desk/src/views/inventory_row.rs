use corten::prelude::*;

use super::{Line, PriceCell};

/// One item: markup only. The grid's refs (`row`, `select`, `qty`) go on this row's
/// elements, so the grid's handlers get every row and checkbox, and every stock cell by its
/// item's id (`key`).
#[component]
pub fn InventoryRow<'a>(
    line: &'a Line<'a>,
    row: Ref<el::HTMLTableRowElement>,
    select: Ref<el::HTMLInputElement>,
    qty: Ref<el::HTMLTableCellElement>,
) -> impl Render {
    let item = line.item;
    html! {
        <tr ref={row} id={("row-", item.id.as_str())}>
            <td>
                <input ref={select} type="checkbox" value={item.id} aria-label={("Select ", item.name.as_str())}>
            </td>
            <td>{item.name}</td>
            <td>{item.set}</td>
            <td>{item.condition}</td>
            <td ref={qty} key={item.id}>{line.qty}</td>
            <td><PriceCell id={&item.id} cents={Some(item.price_cents)} typed="" errors={&Errors::new()} /></td>
        </tr>
    }
}
