use corten::prelude::*;

/// A price cell: tabular figures, so prices line up down a column.
#[component(css)]
pub fn Price<'a>(amount: &'a str) -> impl Render {
    html! { <td class={Amount}>{amount}</td> }
}
