use corten::{
    i18n::{Decimal, Minor},
    prelude::*,
};

use crate::text::{Msg, USD};

/// What the listed stock is worth, by set: the set, how many items, their value and its share of
/// the total, then the total. Numbers are right-aligned with figures of one width, so they line
/// up by place; each share has a bar as long as it, drawn from the number (`style={SHARE.pct(..)}`
/// sets the stylesheet's `--share`, under a CSP that allows no style attribute).
#[component(css)]
pub fn StatementTable<'a>(t: Msg, sets: &'a [(&'a str, usize, u64)]) -> impl Render {
    let items: usize = sets.iter().map(|(_, items, _)| items).sum();
    let cents: u64 = sets.iter().map(|(_, _, cents)| cents).sum();
    // A set's share of the total in thousandths: the text shows one decimal of a percent.
    let share = move |part: u64| (part * 1000).checked_div(cents).unwrap_or(0);
    html! {
        <table class={Table}>
            <thead>
                <tr>
                    <th>"Set"</th>
                    <th>"Items"</th>
                    <th>"Stock value"</th>
                    <th>"Share"</th>
                </tr>
            </thead>
            <tbody>
                for (name, items, part) in sets {
                    let share = share(*part);
                    <tr>
                        <th scope="row">{*name}</th>
                        <td>{t.number(*items)}</td>
                        <td>{t.money(Minor(*part), USD)}</td>
                        <td>
                            <span class={Bar} style={SHARE.pct(share as f64 / 10.0)}></span>
                            {t.percent(Decimal::new(i128::from(share), 3), 1)}
                        </td>
                    </tr>
                }
            </tbody>
            <tfoot>
                <tr>
                    <th scope="row">"Total"</th>
                    <td>{t.number(items)}</td>
                    <td>{t.money(Minor(cents), USD)}</td>
                    <td></td>
                </tr>
            </tfoot>
        </table>
    }
}
