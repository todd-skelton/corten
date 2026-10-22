//! A design system's field: a label and a control, styled by the control's roles and states.
use corten::prelude::*;

/// `id` is the control's: the label's `for`.
#[component(css)]
pub fn Field<'a, C: Render>(id: &'a str, label: &'a str, children: C) -> impl Render {
    html! {
        <div class={Root}>
            <label for={id}>{label}</label>
            <div class={Control}>{children}</div>
        </div>
    }
}
