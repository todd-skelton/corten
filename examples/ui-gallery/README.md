# ui-gallery

A seller's draft listing for one card, built from every [`corten-ui`](../../crates/corten-ui)
component: number fields for the quantity and the shipping weight (the price is a text input read
as money), a combobox for the condition, the regions it
ships to (at least one), the lowest offer it takes (required only when it takes offers), the card's
market in tabs, a menu button, a toast when the draft is saved, a dialog the server draws open
when it refuses a name, a dialog the address opens, a button that copies the SKU, and one that
prints the page. Every part is drawn by the UI kit (`corten ui`), from the kit's own files, so
the page is what an app that ran `corten ui add --all` gets. Its orders page shows the rest of
the kit: a table of orders with their pages, status badges and avatars, an alert, a share popover,
the shipping settings (a select, a slider, a switch and a text area) and the questions sellers ask
in an accordion. Read it when you need a widget the browser has no element for, or to see a kit
component in use.

## Run it

From this directory:

```sh
corten dev     # http://localhost:3000
```

`corten` is the framework's command (`cargo install corten-cli`).

Or in Docker, from the repository root:

```sh
docker build -f examples/ui-gallery/Dockerfile -t corten-ui-gallery .
docker run --rm -p 3000:8080 corten-ui-gallery
```

It needs no environment.

## Where things are

| Piece | File |
|---|---|
| The routes, each form's handler, the layout | [`src/app.rs`](src/app.rs) |
| The draft listing, every corten-ui widget, the two dialogs | [`src/gallery.rs`](src/gallery.rs) |
| The orders page, the kit's other components | [`src/orders.rs`](src/orders.rs) |
| The UI kit: each component from [`crates/corten-cli/kit`](../../crates/corten-cli/kit), and its theme | [`src/ui.rs`](src/ui.rs) |
| The page's own layout, around the kit's components | [`src/gallery.css`](src/gallery.css), [`src/orders.css`](src/orders.css) |

Its browser test runs every component in Chromium, Firefox and WebKit, with and without
JavaScript: [`smoke.mjs`](smoke.mjs).
