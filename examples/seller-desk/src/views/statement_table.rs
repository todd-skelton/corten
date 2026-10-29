use corten::{i18n::Minor, prelude::*};

use crate::text::{Msg, USD};

/// What the listed stock is worth, by set: the set, how many items, and their value, then the
/// total. Numbers are right-aligned with figures of one width, so they line up by place.
#[component(css)]
pub fn StatementTable<'a>(t: Msg, sets: &'a [(&'a str, usize, u64)]) -> impl Render {
    let items: usize = sets.iter().map(|(_, items, _)| items).sum();
    let cents: u64 = sets.iter().map(|(_, _, cents)| cents).sum();
    html! {
        <table class={Table}>
            <thead>
                <tr>
                    <th>"Set"</th>
                    <th>"Items"</th>
                    <th>"Stock value"</th>
                </tr>
            </thead>
            <tbody>
                for (name, items, cents) in sets {
                    <tr>
                        <th scope="row">{*name}</th>
                        <td>{t.number(*items)}</td>
                        <td>{t.money(Minor(*cents), USD)}</td>
                    </tr>
                }
            </tbody>
            <tfoot>
                <tr>
                    <th scope="row">"Total"</th>
                    <td>{t.number(items)}</td>
                    <td>{t.money(Minor(cents), USD)}</td>
                </tr>
            </tfoot>
        </table>
    }
}
