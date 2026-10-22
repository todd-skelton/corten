use corten::{prelude::*, script::el};

/// A modal that asks before an action. Cancel closes it (`method="dialog"`, no script);
/// the confirm button runs the parent's handler, then closes it the same way.
#[component]
pub fn ConfirmDialog<'a, C: Render>(
    dialog: Ref<el::HTMLDialogElement>,
    title: &'a str,
    confirm: &'a str,
    on_confirm: Handler<'a>,
    children: C,
) -> impl Render {
    html! {
        <dialog ref={dialog}>
            <h2>{title}</h2>
            {children}
            <form method="dialog">
                <button value="cancel">"Cancel"</button>
                <button value="confirm" on:click={on_confirm}>{confirm}</button>
            </form>
        </dialog>
    }
}
