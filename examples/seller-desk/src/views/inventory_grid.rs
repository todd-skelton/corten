use corten::prelude::*;

use super::{BulkBar, ConfirmDialog, InventoryRow, Line, RowToggled};
use crate::app::{ArchiveInput, url};

/// Every item, with selection (click, shift-click for a range, select all), a filter, sortable
/// columns, inline price edits, a confirm dialog for archiving, and live stock levels.
#[component(css, js)]
pub fn InventoryGrid<'a>(lines: &'a [Line<'a>]) -> impl Render {
    let (body, row, select, all, qty, count, bar, dialog, form, ids, empty) = refs();
    let f = ArchiveInput::FIELDS;
    html! {
        <section class={Grid} on:connect={js::live(qty, url::Levels, signal)}>
            <div class={Toolbar}>
                <input
                    type="search"
                    placeholder="Filter by name or set"
                    aria-label="Filter"
                    on:input={js::filter(target, row, empty, select, all, count, bar)}
                >
                <BulkBar
                    count={count}
                    bar={bar}
                    on:archive={js::confirm(dialog)}
                    on:clear={js::clear(scope, select, row, all, count, bar)}
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
                                on:change={js::selectAll(scope, row, select, all, count, bar)}
                            >
                        </th>
                        <th><button type="button" on:click={js::sort(target, body, 1)}>"Name"</button></th>
                        <th><button type="button" on:click={js::sort(target, body, 2)}>"Set"</button></th>
                        <th>"Condition"</th>
                        <th><button type="button" on:click={js::sort(target, body, 4)}>"Qty"</button></th>
                        <th><button type="button" on:click={js::sort(target, body, 5)}>"Price"</button></th>
                    </tr>
                </thead>
                <tbody ref={body} on:RowToggled={js::toggled(event, scope, row, select, all, count, bar)}>
                    for line in lines {
                        <InventoryRow line={line} row={row} select={select} qty={qty} />
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
                on:confirm={js::archive(scope, select, row, all, form, ids, count, bar)}
            >
                <p>"Archived items leave the grid. Their listings end."</p>
            </ConfirmDialog>
        </section>
    }
}
