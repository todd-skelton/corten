# seller-desk

A seller's inventory screen: a grid of 500 cards with selection, shift-click ranges, select all,
filter, sort, inline price edits, a confirm dialog before archiving, live stock levels, and a
statement of what the stock is worth. It is the client-heavy example: everything the grid does in
the browser is a function its markup calls, with no client framework. Read it when your screen does
more in the browser than forms and links.

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
| The layout and the status line | [`src/views.rs`](src/views.rs) |
| The grid: selection, ranges, filter, sort, live levels; a row; the bulk bar | [`src/views/inventory_grid.rs`](src/views/inventory_grid.rs), [`src/views/inventory_grid.js`](src/views/inventory_grid.js), [`src/views/inventory_row.rs`](src/views/inventory_row.rs), [`src/views/bulk_bar.rs`](src/views/bulk_bar.rs) |
| A price cell edited in place | [`src/views/price_cell.rs`](src/views/price_cell.rs), [`src/views/price_cell.js`](src/views/price_cell.js) |
| The confirm dialog (the platform's `<dialog>`, no script) | [`src/views/confirm_dialog.rs`](src/views/confirm_dialog.rs) |
| The statement | [`src/views/statement_table.rs`](src/views/statement_table.rs) |

Its browser test, which also times the grid: [`smoke.mjs`](smoke.mjs).
