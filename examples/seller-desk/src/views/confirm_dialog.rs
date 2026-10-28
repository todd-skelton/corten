use corten::prelude::*;

/// A modal that asks before an action. A button opens it with `command="show-modal"` and
/// `commandfor={id}`; Cancel closes it (`method="dialog"`), all without a script. The confirm
/// button runs the parent's handler, then closes it the same way.
#[component]
pub fn ConfirmDialog<'a, I: Id, C: Render>(
    id: I,
    title: &'a str,
    confirm: &'a str,
    on_confirm: Handler<'a>,
    children: C,
) -> impl Render {
    html! {
        <dialog id={id}>
            <h2>{title}</h2>
            {children}
            <form method="dialog">
                <button value="cancel">"Cancel"</button>
                <button value="confirm" on:click={on_confirm}>{confirm}</button>
            </form>
        </dialog>
    }
}
