use corten::{i18n::Minor, prelude::*};

use crate::text::{Msg, USD};

/// A price cell: tabular figures, so prices line up down a column. A price in US dollars is
/// written as the reader's locale writes money; any other is shown as the item API sent it.
#[component(css)]
pub fn Price<'a>(t: Msg, cents: Option<u64>, sent: &'a str) -> impl Render {
    html! {
        <td class={Amount}>
            match cents {
                Some(cents) => { {t.money(Minor(cents), USD)} }
                None => { {sent} }
            }
        </td>
    }
}
