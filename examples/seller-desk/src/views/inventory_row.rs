use corten::{prelude::*, script::el};

use super::{Line, PriceCell};

/// One item. The checkbox tells the grid (`RowToggled`); the grid's refs (`row`, `select`,
/// `qty`) are put on this row's elements, so the grid's handlers get every row, checkbox and
/// stock cell.
#[component(js)]
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
                <input
                    ref={select}
                    type="checkbox"
                    value={item.id}
                    aria-label={("Select ", item.name.as_str())}
                    on:click={js::toggle(event, target, scope, &item.id)}
                >
            </td>
            <td>{item.name}</td>
            <td>{item.set}</td>
            <td>{item.condition}</td>
            <td ref={qty} data-id={item.id}>{line.qty}</td>
            <td><PriceCell id={&item.id} cents={Some(item.price_cents)} typed="" errors={&Errors::new()} /></td>
        </tr>
    }
}
