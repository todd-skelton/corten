use std::{sync::Arc, time::SystemTime};

use corten::{
    i18n::{Minor, RelativeStyle},
    prelude::*,
};
use corten_ui::RelativeTime;

use crate::{
    app::{house, url},
    store::Standing,
    text::{Msg, USD},
};

/// The house's desk: every lot, and a button that ends an open one now (its top bidder wins).
#[component(css)]
pub fn HouseDesk<'a>(t: Msg, lots: &'a [Arc<Standing>], now: SystemTime) -> impl Render {
    html! {
        <h1>"House"</h1>
        <p class={Note}>"Ending a lot closes it now. Its top bidder wins and is told."</p>
        <table class={Desk}>
            <thead>
                <tr>
                    <th>"Lot"</th>
                    <th>"Price"</th>
                    <th>"Bids"</th>
                    <th>"Ends"</th>
                    <th><span class={Hidden}>"Action"</span></th>
                </tr>
            </thead>
            <tbody>
                for standing in lots {
                    let open = !standing.ended(now);
                    <tr>
                        <td><a href={url::Lot { id: &standing.lot.id }}>{&standing.lot.card}</a></td>
                        <td>{t.money(Minor(standing.top().map_or(standing.lot.opening_cents, |bid| bid.cents)), USD)}</td>
                        <td>{t.number(standing.count)}</td>
                        <td>
                            if open {
                                <RelativeTime when={t.relative(standing.lot.ends_at, now, RelativeStyle::Short)} />
                            } else {
                                "Closed"
                            }
                        </td>
                        <td>
                            if open {
                                <form method="post" action={house::End { id: &standing.lot.id }}>
                                    <button>"End now"<span class={Hidden}>": "{&standing.lot.card}</span></button>
                                </form>
                            }
                        </td>
                    </tr>
                }
            </tbody>
        </table>
    }
}
