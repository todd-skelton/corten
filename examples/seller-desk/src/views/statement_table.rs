use corten::{
    i18n::{Decimal, Minor},
    prelude::*,
};

use crate::{
    text::{Msg, USD},
    ui::table::{Table, TableNumeric},
};

/// What the listed stock is worth, by set: the set, how many items, their value and its share of
/// the total, then the total, in the kit's table. Numbers are right-aligned with figures of one
/// width (`TableNumeric`), so they line up by place; each share has a bar as long as it, drawn from the number (`style={SHARE.pct(..)}`
/// sets the stylesheet's `--share`, under a CSP that allows no style attribute).
#[component(css)]
pub fn StatementTable<'a>(t: Msg, sets: &'a [(&'a str, usize, u64)]) -> impl Render {
    let items: usize = sets.iter().map(|(_, items, _)| items).sum();
    let cents: u64 = sets.iter().map(|(_, _, cents)| cents).sum();
    // A set's share of the total in thousandths: the text shows one decimal of a percent.
    // In u128, where a value in cents times 1,000 cannot overflow.
    let share = move |part: u64| (u128::from(part) * 1000).checked_div(u128::from(cents)).unwrap_or(0);
    html! {
        <div class={Surface}>
            <Table>
                <table>
                    <thead>
                        <tr>
                            <th scope="col">"Set"</th>
                            <th scope="col" class={TableNumeric {}}>"Items"</th>
                            <th scope="col" class={TableNumeric {}}>"Stock value"</th>
                            <th scope="col" class={TableNumeric {}}>"Share"</th>
                        </tr>
                    </thead>
                    <tbody>
                        for (name, items, part) in sets {
                            let share = share(*part);
                            <tr>
                                <th scope="row" class={Set}>{*name}</th>
                                <td class={TableNumeric {}}>{t.number(*items)}</td>
                                <td class={TableNumeric {}}>{t.money(Minor(*part), USD)}</td>
                                <td class={TableNumeric {}}>
                                    <span class={Bar} style={SHARE.pct(share as f64 / 10.0)}></span>
                                    {t.percent(Decimal::new(share as i128, 3), 1)}
                                </td>
                            </tr>
                        }
                    </tbody>
                    <tfoot>
                        <tr>
                            <th scope="row">"Total"</th>
                            <td class={TableNumeric {}}>{t.number(items)}</td>
                            <td class={TableNumeric {}}>{t.money(Minor(cents), USD)}</td>
                            <td></td>
                        </tr>
                    </tfoot>
                </table>
            </Table>
        </div>
    }
}
