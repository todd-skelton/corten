use corten::prelude::*;

use super::{BulkBar, ConfirmDialog, InventoryRow, Line, PriceEdited};
use crate::{
    app::{ArchiveInput, url},
    text::{Msg, USD},
};

/// The archive form's id: the rows' checkboxes name it in `form=`.
#[derive(Id)]
pub struct ArchiveFormId;

/// The archive dialog's id: the bulk bar's Archive button opens it (`commandfor`), no script.
#[derive(Id)]
pub struct ArchiveDialogId;

/// Every item, with selection (click, shift-click for a range, select all), a filter, sortable
/// columns, inline price edits (each cell says whether its price is saved, a `PriceEdited`
/// event, and the grid counts those that are not), a confirm dialog for archiving, and live stock
/// levels. The sort and the filter read the rows' fields by name (`row.name`, `row.qty`), so a
/// column moved in the markup cannot sort the wrong one, and the count of unsaved prices is the
/// catalog's text (`t.js()`).
///
/// The rows are `InventoryRow` views with no handlers of their own: one handler on the `<tbody>`
/// gets the checkbox that was clicked (`select.closest()`), so 500 rows cost one `data-on`, and
/// the script writes a row's holes through `rows(body, InventoryRow)`.
#[component(css, js)]
pub fn InventoryGrid<'a>(lines: &'a [Line<'a>], t: Msg) -> impl Render {
    let (body, select, all, count, bar, form, empty, note) = refs();
    // What the selection handlers update, built once and passed as one argument.
    let sel = js::Selection { body, all, count, bar };
    // The unsaved-prices note and its text, one message per plural form.
    let unsent = js::Unsent { note, one: t.js().grid_unsent_one(), other: t.js().grid_unsent_other() };
    html! {
        <section
            class={Grid}
            on:connect={js::live(body, url::Levels, signal)}
            on:PriceEdited={js::priceEdited(event, body, unsent)}
            on:submit={js::priceSent(event, body, unsent)}
        >
            <div class={Toolbar}>
                <input
                    type="search"
                    placeholder="Filter by name or set"
                    aria-label="Filter"
                    on:input={js::filter(target, empty, sel)}
                >
                <BulkBar count={count} bar={bar} archive={ArchiveDialogId} on:clear={js::clear(sel)} />
                <output ref={note} hidden></output>
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
                                    on:change={js::selectAll(sel)}
                                >
                            </th>
                            <th><button type="button" on:click={js::sortByName(target, body)}>"Name"</button></th>
                            <th><button type="button" on:click={js::sortBySet(target, body)}>"Set"</button></th>
                            <th>"Condition"</th>
                            <th><button type="button" on:click={js::sortByQty(target, body)}>"Qty"</button></th>
                            <th>
                                <button type="button" on:click={js::sortByPrice(target, body)}>"Price ("{USD.code()}")"</button>
                            </th>
                        </tr>
                    </thead>
                    <tbody ref={body} on:click={js::toggle(event, select.closest(), sel)}>
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
                id={ArchiveDialogId}
                title="Archive the selected items?"
                confirm="Archive"
                on:confirm={js::archive(form, sel)}
            >
                <p>"Archived items leave the grid. Their listings end."</p>
            </ConfirmDialog>
        </section>
    }
}
