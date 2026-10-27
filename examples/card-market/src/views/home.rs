use corten::prelude::*;

use crate::{routes, text::Msg};

/// The home page: links to the first items.
#[component(css)]
pub fn Home(t: Msg) -> impl Render {
    html! {
        <h1>{t.home_title()}</h1>
        <ul class={List}>
            for n in 1..=50 {
                let id = format!("item_{n}");
                <li><a href={routes::Item { id: &id }}>"Card Name "{n}</a></li>
            }
        </ul>
    }
}
