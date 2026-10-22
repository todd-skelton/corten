# Islands

[Back to the guides](../../README.md#documentation)

An island is a component with behavior. Its markup is Rust like every other view; its behavior
is a script in the file next to it. The markup names what the script touches: `ref={Rows}` is an
element the script uses, `on:click={ShowAll}` runs one of its actions. Everything else in the
script is plain JavaScript.

```rust,ignore
// src/views/offer_list.rs
#[island]
pub fn OfferList<'a>(t: Msg, offers: &'a [Offer]) -> impl Render {
    let (first, rest) = offers.split_at(offers.len().min(5));
    html! {
        <table><tbody ref={Rows}>for o in first { <OfferRow o={o} /> }</tbody></table>
        if !rest.is_empty() {
            <template ref={Rest}>for o in rest { <OfferRow o={o} /> }</template>
            <button type="button" on:click={ShowAll}>{t.item_show_all_offers(offers.len())}</button>
        }
    }
}
```

```js
// src/views/offer_list.js
import { island } from "corten";

export default island("offer-list", {
  actions: {
    showAll({ refs, target }) {
      refs.rows.append(refs.rest.content);
      refs.rest.remove();
      target.remove();
    },
  },
});
```

| In the markup | In the script |
|---|---|
| `ref={Rows}` on a `<tbody>` | `refs.rows`, an `HTMLTableSectionElement` (`all.rows` for every one) |
| `on:click={ShowAll}` on a `<button>` | `actions.showAll(ctx)`, with `ctx.target` the button and `ctx.event` the click |
| a parameter marked `#[prop]` (feature `json`) | `props.name`, typed from the Rust type |
| the component, `<OfferList ... />` | `<offer-list>` around the markup; `state()` per element, `connect(ctx)` when it joins the page |

The component renders inside its tag (its name in kebab-case) and works without the script. The
script is compiled into the binary; list the island once, `App::new(routes).components(&[OfferList])`,
and Corten serves it at a versioned URL and preloads it on the pages that render it. The
generated declarations (`Types::components`, see [Type safety](type-safety.md)) describe each island's markup, so
`island("offer-list", ...)` is type-checked with no imports: a ref the markup does not have, a
missing or misspelled action, or `.content` on a `<tbody>` fails `npm run check`. In Rust, a
ref or action outside its island, or as a string, does not compile.

`ref` and `on:` are the only island syntax, and there are no built-in client behaviors to learn:
showing, toggling or reordering is an action of a few lines in the island's script.

## Rules

1. HTML comes from the server, in `html!`. Islands never build markup from strings; they move,
   show or hide server-rendered nodes. Extra rows ship in a `<template ref={Rest}>`.
2. An island's script reaches the page only through the names its markup gives it: `refs`,
   `all` and actions run by `on:event`. Per-element state is `state()` or the DOM itself.
3. User-facing text lives in views or the view model, never in island code.
4. Cleanup is `ctx.signal` alone: pass it to `addEventListener`, `fetch`, `subscribe` and
   `mount`. `connect` returns nothing. See [Cancellation](cancellation.md).

## Screens with client state

For screens whose state lives in the browser (a grid of thousands of live rows), an island can
render with Solid and keep its rows current from a [feed](live-data.md):

| Piece | Server | Browser |
|---|---|---|
| Typed props | `#[prop] grid: &GridProps` (feature `json`) | `props.grid` |
| Live rows | `corten::feed::Feed<Row>`: `write(\|tx\| ...)`, `snapshot()`, `live(since)` (feature `feed`) | `subscribe(props.feed, { snapshot, patch, status }, { signal })` in `connect` |
| Rendering | serves Solid as `corten/solid` (feature `solid`) | `mount(el, (server) => h(Grid, ...), { signal })` in `connect` |
