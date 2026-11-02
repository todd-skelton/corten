# seller-desk

A seller's inventory screen: a grid of 500 cards with selection, shift-click ranges, select all,
filter, sort, inline price edits, a confirm dialog before archiving, live stock levels, and a
statement of what the stock is worth. It is the client-heavy example: everything the grid does in
the browser is a function its markup calls, with no client framework. Read it when your screen does
more in the browser than forms and links.

It is also the example styled with the UI kit as `corten ui` gives it: the bulk bar's and dialog's
buttons, the search field, the dialog and the statement table are the kit's components, copied
into [`src/ui/`](src/ui) and unedited, and the kit's theme tokens take the desk's teal and radius in
[`tokens/`](tokens) (light and dark). Two parts of the grid write a kit look in their own CSS: the
table, because the kit's table scrolls sideways and the grid's header stays as the rows scroll,
and the price cell, because the kit's classes on 500 of them made the page slower to render
(the numbers are in [`price_cell.css`](src/views/price_cell.css)).

This repository formats wider than rustfmt's defaults, so [`src/ui.rs`](src/ui.rs) differs from
what `corten ui` writes: a comment says why, a `#[rustfmt::skip]` on each module keeps the copies as the
kit lays them out, and `COMPONENTS` is on one line. After a `corten ui add` here, give the new
module its own skip line (and the module it was put under its own back) and run `cargo fmt`.

## Run it

From this directory:

```sh
corten dev     # http://localhost:3000
```

`corten` is the framework's command (`cargo install corten-cli`).

Or in Docker, from the repository root:

```sh
docker build -f examples/seller-desk/Dockerfile -t corten-seller-desk .
docker run --rm -p 3000:8080 corten-seller-desk
```

`DESK_ROWS` sets how many rows the grid has (500).

## Where things are

The routes and what each shows are in the table at the top of [`src/app.rs`](src/app.rs).

| Piece | File |
|---|---|
| The routes, the stock, the archive (once per submit key), the `Server-Timing` middleware | [`src/app.rs`](src/app.rs) |
| The layout, the status line and the item count | [`src/views.rs`](src/views.rs) |
| The grid: selection, ranges, filter, sort, live levels and sold-out rows, a header that stays as the rows scroll; a row; the bulk bar | [`src/views/inventory_grid.rs`](src/views/inventory_grid.rs), [`src/views/inventory_grid.js`](src/views/inventory_grid.js), [`src/views/inventory_row.rs`](src/views/inventory_row.rs), [`src/views/bulk_bar.rs`](src/views/bulk_bar.rs) |
| A price cell edited in place | [`src/views/price_cell.rs`](src/views/price_cell.rs), [`src/views/price_cell.js`](src/views/price_cell.js) |
| The confirm dialog (the platform's `<dialog>`, no script) | [`src/views/confirm_dialog.rs`](src/views/confirm_dialog.rs) |
| The UI kit: the copied components (`corten ui diff` says they are unedited) and the theme recolored to the desk's palette | [`src/ui.rs`](src/ui.rs), [`src/ui/`](src/ui), [`tokens/ui.tokens.json`](tokens/ui.tokens.json), [`tokens/ui-dark.tokens.json`](tokens/ui-dark.tokens.json) |
| The statement, with its total | [`src/views/statement_table.rs`](src/views/statement_table.rs) |

Its browser test, which also times the grid: [`smoke.mjs`](smoke.mjs).
