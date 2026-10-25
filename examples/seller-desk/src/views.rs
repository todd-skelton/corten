//! The desk's views. Each component's script and stylesheet are next to it in `views/`.
use corten::{Component, prelude::*};

use crate::app::Item;

mod bulk_bar;
mod confirm_dialog;
mod inventory_grid;
mod inventory_row;
mod price_cell;

pub use bulk_bar::BulkBar;
pub use confirm_dialog::ConfirmDialog;
pub use inventory_grid::{ArchiveFormId, InventoryGrid};
pub use inventory_row::{InventoryRow, RowId};
pub use price_cell::PriceCell;

/// What the app lists in `App::components`: the scripts, the stylesheets and the row's view.
pub const COMPONENTS: &[Component] = &[InventoryGrid, InventoryRow, PriceCell];

/// An item and its stock level, as a row shows them.
pub struct Line<'a> {
    pub item: &'a Item,
    pub qty: u32,
}

/// The document around every page.
#[component]
pub fn Layout<'a, C: Render>(req: &'a Req, title: &'a str, children: C) -> impl Document + 'a {
    html! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <link rel="stylesheet" href={req.asset("style.css")}>
                <title>{title}</title>
                <Corten />
            </head>
            <body>
                <main>{children}</main>
            </body>
        </html>
    }
}

pub fn desk<'a>(lines: &'a [Line<'a>]) -> impl Render + 'a {
    html! {
        <h1>"Inventory"</h1>
        <StatusLine text="" />
        <InventoryGrid lines={lines} />
    }
}

/// The status line's id: bulk actions patch it.
#[derive(Id)]
pub struct DeskStatusId;

/// What the last bulk action did; the server patches it by `id`.
#[component]
pub fn StatusLine<'a>(text: &'a str) -> impl Render {
    html! { <p id={DeskStatusId} role="status">{text}</p> }
}

/// `1234` as `$12.34`.
pub fn money(cents: u32) -> String {
    format!("{}.{:02}", cents / 100, cents % 100)
}
