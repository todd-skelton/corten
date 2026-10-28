# ui-gallery

A seller's draft listing for one card, built from every [`corten-ui`](../../crates/corten-ui)
component: a number field for the quantity and the price, a combobox for the condition, the
card's market in tabs, a menu button, a toast when the draft is saved, a dialog the server draws
open when it refuses a name, and a dialog the address opens. Read it when you need a widget the
browser has no element for.

## Run it

From this directory:

```sh
corten dev     # http://localhost:3000
```

Or in Docker, from the repository root:

```sh
docker build -f examples/ui-gallery/Dockerfile -t corten-ui-gallery .
docker run --rm -p 3000:8080 corten-ui-gallery
```

It needs no environment.

## Where things are

| Piece | File |
|---|---|
| The routes, each form's handler | [`src/app.rs`](src/app.rs) |
| The screen, every component, the two dialogs | [`src/gallery.rs`](src/gallery.rs) |
| The small design system that styles the headless parts by role and state | [`src/gallery.css`](src/gallery.css), [`src/field.rs`](src/field.rs), [`src/field.css`](src/field.css) |

Its browser test runs every component in Chromium, Firefox and WebKit, with and without
JavaScript: [`smoke.mjs`](smoke.mjs).
