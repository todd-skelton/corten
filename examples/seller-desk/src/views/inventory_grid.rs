use corten::prelude::*;

use super::{BulkBar, ConfirmDialog, InventoryRow, Line, PriceEdited};
use crate::app::{ArchiveInput, url};

/// The archive form's id: the rows' checkboxes name it in `form=`.
#[derive(Id)]
pub struct ArchiveFormId;

/// Every item, with selection (click, shift-click for a range, select all), a filter, sortable
/// columns, inline price edits (each cell says whether its price is saved, a `PriceEdited`
/// event, and the grid counts those that are not), a confirm dialog for archiving, and live stock
/// levels.
///
/// The rows are `InventoryRow` views with no handlers of their own: one handler on the `<tbody>`
/// gets the checkbox that was clicked (`select.closest()`), so 500 rows cost one `data-on`, and
/// the script writes a row's holes through `rows(body, InventoryRow)`.
#[component(css, js)]
pub fn InventoryGrid<'a>(lines: &'a [Line<'a>]) -> impl Render {
    let (body, select, all, count, bar, dialog, form, empty, unsent) = refs();
    // What the selection handlers update, built once and passed as one argument.
    let sel = js::Selection { body, all, count, bar };
    html! {
        <section
            class={Grid}
            on:connect={js::live(body, url::Levels, signal)}
            on:PriceEdited={js::priceEdited(event, scope, body, unsent)}
            on:submit={js::priceSent(event, scope, body, unsent)}
        >
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
                <output ref={unsent} hidden></output>
            </div>
            <div class={Scroll}>
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
            </div>
            <p ref={empty} class={Empty} hidden>"No items match the filter."</p>
            // Each row's checkbox belongs to this form (`form=`), so it posts the checked ids. Its
            // submit key makes a resend (a lost answer, a retry) archive once.
            <form ref={form} id={ArchiveFormId} method="post" action={url::Archive} data-patch hidden>
                {SubmitKey::field(ArchiveInput::FIELDS.submit)}
            </form>
            <ConfirmDialog
                dialog={dialog}
                title="Archive the selected items?"
                confirm="Archive"
                on:confirm={js::archive(scope, form, sel)}
            >
                <p>"Archived items leave the grid. Their listings end."</p>
            </ConfirmDialog>
        </section>
    }
}
