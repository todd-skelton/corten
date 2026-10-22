# Islands, live fragments and feeds

## An island

The markup is an `#[island]` component; the behavior is `snake_name.js` next to it.

```rust
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
// @ts-check
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

1. The tag is the component name in kebab-case. `ref={Rows}` is `refs.rows` (`all.rows` for
   every match); `on:click={ShowAll}` runs `actions.showAll`. Any DOM event works: `on:input`,
   `on:keydown` and so on.
2. An action gets `{ el, refs, all, props, state, signal, target, event }`. `state: () => ({..})`
   is per element; `connect(ctx)` runs when the element joins the page.
3. `#[prop] feed: &'a str` (feature `json`) is `props.feed` in the script, typed from Rust.
4. List the island in `App::components`. Declarations generated from the markup (see
   [testing.md](testing.md)) let `tsc` or the editor check the script: a missing ref or action
   is an error.
5. A control that does nothing without the script renders `hidden`, and `connect` shows it.

## A fragment the server keeps current

No script needed. Stream components with an `id`; the runtime swaps each one in, like a patch:

```rust
// in the page
<corten-live src={Trades}><LastTrade text={&last} /></corten-live>

// GET /live/trades
async fn trades(State(m): State<Market>) -> impl IntoResponse {
    let next = m.trades.subscribe();                       // a tokio watch channel
    corten::live(futures_util::stream::unfold(next, |mut next| async move {
        next.changed().await.ok()?;
        let html = LastTrade { text: &next.borrow_and_update() }.to_html();
        Some((html, next))
    }))
}
```

## Live rows in an island (feature `feed`)

```rust
// a Feed<Stock> where Stock: Row (has an id). Routes:
.route(Stocks::PATH, get(|State(m): State<Market>| async move { m.stocks.snapshot() }))
.route(StocksLive::PATH, get(|State(m): State<Market>, since: Since| async move { m.stocks.live(since) }))
// writes: m.stocks.write(|tx| { let s = tx.get_mut(&id)?; s.set_price(cents); Some(()) });
```

```js
connect({ all, props, signal }) {
  subscribe(props.feed, {
    snapshot: (rows) => rows.forEach((row) => paint(all.price, row)),
    patch: (changes) => { for (const c of changes) if (c.op === "upsert") paint(all.price, c.value); },
  }, { signal });
},
```

The server renders the rows first; the feed only keeps them current. Clients resume from their
last patch after a disconnect. Across several instances, use `corten-cluster`.

## Heavy client state (feature `solid`)

For a grid of thousands of live rows, render with Solid inside the island:
`import { h, mount, createStore } from "corten/solid"`, then in `connect` call
`mount(el, () => h(Grid, ...), { signal })` and feed it from `subscribe`. Use it only where the
plain DOM pattern above gets complicated.

## Widgets

Use `corten-ui` (number field, combobox, tabs, menu button) before writing one. For anything
else, use the platform first: `<dialog>`, `popover`, `<details>`, and CSS anchor positioning.
