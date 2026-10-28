use corten::{prelude::*, script::el};

/// The bulk actions, shown while rows are selected. The grid owns the selection, so it passes
/// the elements it updates (`count`, `bar`), the dialog Archive opens (`archive`, by the
/// platform's `commandfor`) and what Clear does.
#[component]
pub fn BulkBar<'a, A: Id>(
    count: Ref<el::HTMLOutputElement>,
    bar: Ref<el::HTMLDivElement>,
    archive: A,
    on_clear: Handler<'a>,
) -> impl Render {
    html! {
        <div ref={bar} role="group" aria-label="Selected items" hidden>
            <output ref={count}>"0"</output>" selected "
            <button type="button" command="show-modal" commandfor={archive}>"Archive"</button>
            <button type="button" on:click={on_clear}>"Clear"</button>
        </div>
    }
}
