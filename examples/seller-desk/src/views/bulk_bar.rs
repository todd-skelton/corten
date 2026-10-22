use corten::{prelude::*, script::el};

/// The bulk actions, shown while rows are selected. The grid owns the selection, so it passes
/// the elements it updates (`count`, `bar`) and decides what each button does.
#[component]
pub fn BulkBar<'a>(
    count: Ref<el::HTMLOutputElement>,
    bar: Ref<el::HTMLDivElement>,
    on_archive: Handler<'a>,
    on_clear: Handler<'a>,
) -> impl Render {
    html! {
        <div ref={bar} role="group" aria-label="Selected items" hidden>
            <output ref={count}>"0"</output>" selected "
            <button type="button" on:click={on_archive}>"Archive"</button>
            <button type="button" on:click={on_clear}>"Clear"</button>
        </div>
    }
}
