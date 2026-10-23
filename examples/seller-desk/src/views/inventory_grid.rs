use corten::prelude::*;

use super::{BulkBar, ConfirmDialog, InventoryRow, Line};
use crate::app::{ArchiveInput, url};

/// Every item, with selection (click, shift-click for a range, select all), a filter, sortable
/// columns, inline price edits, a confirm dialog for archiving, and live stock levels.
///
/// The rows are `InventoryRow` views with no handlers of their own: one handler on the `<tbody>`
/// gets the checkbox that was clicked (`select.closest()`), so 500 rows cost one `data-on`, and
/// the script writes a row's holes through `rows(body, InventoryRow)`.
#[component(css, js)]
pub fn InventoryGrid<'a>(lines: &'a [Line<'a>]) -> impl Render {
    let (body, select, all, count, bar, dialog, form, ids, empty) = refs();
    // What the selection handlers update, built once and passed as one argument.
    let sel = js::Selection { body, all, count, bar };
    let f = ArchiveInput::FIELDS;
    html! {
        <section class={Grid} on:connect={js::live(body, url::Levels, signal)}>
            <div class={Toolbar}>
                <input
                    type="search"
                    placeholder="Filter by name or set"
                    aria-label="Filter"
                    on:input={js::filter(target, empty, sel)}
                >
                <BulkBar
                    count={count}
                    bar={bar}
                    on:archive={js::confirm(dialog)}
                    on:clear={js::clear(scope, sel)}
                />
            </div>
            <table class={Table}>
                <thead>
                    <tr>
                        <th>
                            <input
                                ref={all}
                                type="checkbox"
                                aria-label="Select all"
                                on:change={js::selectAll(scope, sel)}
                            >
                        </th>
                        <th><button type="button" on:click={js::sort(target, body, 1)}>"Name"</button></th>
                        <th><button type="button" on:click={js::sort(target, body, 2)}>"Set"</button></th>
                        <th>"Condition"</th>
                        <th><button type="button" on:click={js::sort(target, body, 4)}>"Qty"</button></th>
                        <th><button type="button" on:click={js::sort(target, body, 5)}>"Price"</button></th>
                    </tr>
                </thead>
                <tbody ref={body} on:click={js::toggle(event, select.closest(), scope, sel)}>
                    for line in lines {
                        <InventoryRow
                            id={&line.item.id}
                            name={&line.item.name}
                            set={&line.item.set}
                            condition={line.item.condition}
                            qty={line.qty}
                            price_cents={line.item.price_cents}
                            selected={false}
                            select={select}
                        />
                    }
                </tbody>
            </table>
            <p ref={empty} class={Empty} hidden>"No items match the filter."</p>
            <form ref={form} method="post" action={url::Archive} data-patch hidden>
                <input ref={ids} type="hidden" name={f.ids}>
            </form>
            <ConfirmDialog
                dialog={dialog}
                title="Archive the selected items?"
                confirm="Archive"
                on:confirm={js::archive(scope, form, ids, sel)}
            >
                <p>"Archived items leave the grid. Their listings end."</p>
            </ConfirmDialog>
        </section>
    }
}
