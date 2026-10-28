use corten::prelude::*;

use crate::routes;

/// The bar above every page: the app's name, a link home, lined up with the page below it.
#[component(css)]
pub fn SiteHeader() -> impl Render {
    html! {
        <header class={Bar}><a href={routes::Home}>"Card Market"</a></header>
    }
}
