use corten::{i18n::Minor, prelude::*};

use crate::text::{Msg, USD};

/// What the listed stock is worth, by set: the set, how many items, and their value. Numbers
/// are right-aligned with figures of one width, so they line up by place.
#[component(css)]
pub fn StatementTable<'a>(t: Msg, sets: &'a [(&'a str, usize, u64)]) -> impl Render {
    html! {
        <table class={Table}>
            <thead>
                <tr>
                    <th>{t.statement_set()}</th>
                    <th>{t.statement_items()}</th>
                    <th>{t.statement_value()}</th>
                </tr>
            </thead>
            <tbody>
                for (name, items, cents) in sets {
                    <tr>
                        <td>{*name}</td>
                        <td>{t.number(*items)}</td>
                        <td>{t.money(Minor(*cents), USD)}</td>
                    </tr>
                }
            </tbody>
        </table>
    }
}
