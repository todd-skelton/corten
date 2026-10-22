//! The desk's views. Each component's script and stylesheet are next to it in `views/`.
use corten::{Component, prelude::*, script::event};

use crate::app::Item;

mod bulk_bar;
mod confirm_dialog;
mod inventory_grid;
mod inventory_row;
mod price_cell;

pub use bulk_bar::BulkBar;
pub use confirm_dialog::ConfirmDialog;
pub use inventory_grid::InventoryGrid;
pub use inventory_row::InventoryRow;
pub use price_cell::PriceCell;

/// What the app lists in `App::components`: the scripts, the stylesheets and the typed events.
pub const COMPONENTS: &[Component] = &[InventoryGrid, InventoryRow, PriceCell, RowToggled];

/// A row's checkbox changed: the row tells the grid, which keeps the selection.
#[event]
pub struct RowToggled {
    pub id: String,
    pub selected: bool,
    /// Shift was held: select the range from the last toggled row.
    pub shift: bool,
}

/// An item and its stock level, as a row shows them.
pub struct Line<'a> {
    pub item: &'a Item,
    pub qty: u32,
}

pub fn shell(doc: Document) -> impl Render {
    html! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <link rel="stylesheet" href={doc.asset("style.css")}>
                {doc.head}
            </head>
            <body>
                <main>{doc.main}</main>
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

/// What the last bulk action did; the server patches it by `id`.
#[component]
pub fn StatusLine<'a>(text: &'a str) -> impl Render {
    html! { <p id="desk-status" role="status">{text}</p> }
}

/// `1234` as `$12.34`.
pub fn money(cents: u32) -> String {
    format!("{}.{:02}", cents / 100, cents % 100)
}
