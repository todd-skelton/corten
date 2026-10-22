# Scripts, live fragments and feeds

Full guide: `docs/guide/scripts.md` in the Corten repository.

## A component with behavior

`#[component(js)]` reads `snake_name.js` next to the component. Its exported functions are the
handlers; markup calls them through the generated `js` module.

```rust
// src/views/offer_list.rs
#[component(js)]
pub fn OfferList<'a>(t: Msg, offers: &'a [Offer]) -> impl Render {
    let (rows, rest) = refs();
    let (first, more) = offers.split_at(offers.len().min(5));
    html! {
        <div>
            <table>
                <tbody ref={rows}>for o in first { <OfferRow o={o} /> }</tbody>
            </table>
            if !more.is_empty() {
                <template ref={rest}>for o in more { <OfferRow o={o} /> }</template>
                <button type="button" on:click={js::showAll(rows, rest, target)}>
                    {t.item_show_all_offers(offers.len())}
                </button>
            }
        </div>
    }
}
```

```js
// src/views/offer_list.js
// @ts-check

/**
 * @param {HTMLTableSectionElement} rows
 * @param {HTMLTemplateElement} rest
 * @param {HTMLButtonElement} button
 */
export function showAll(rows, rest, button) {
  rows.append(rest.content);
  rest.remove();
  button.remove();
}
```

1. `let (a, b) = refs();` declares refs. A ref's element type comes from the tag it is on; a
   parameter typed `HTMLX[]` gets every element with that ref.
2. Arguments a handler can take: refs, `event`, `target` (the element with the `on:`),
   `signal` (aborts when that element leaves the page), `scope` (the component's root), and any
   Rust value (`&str`, numbers, `bool`, a typed route, `Serialize` with feature `json`).
3. A wrong ref type, wrong event type, wrong value type or a handler the script does not export
   is a compile error at the call. `tsc --checkJs` checks the function bodies.
4. `on:connect={..}` runs when the element joins the page: first load, navigation, a patch.
5. A component with refs, or whose handlers take `scope`, has one root element; refs resolve
   inside it and outside nested components. Any element can be the root, a `<tr>` included.
6. A ref on several kinds of element: `ref={tab as HTMLElement}`.
7. State that outlives one event lives in the DOM or in `state(scope, () => init)` from
   `corten`. Handlers are shared by every instance, so never keep state in module variables.
8. A control that does nothing without the script renders `hidden`; an `on:connect` handler
   shows it.
9. List the component in `App::components`.

## A child that reports to its parent

A handler prop: the child takes `on_confirm: Handler<'a>` and puts it on an element with
`on:click={on_confirm}`; the parent writes `<ConfirmButton on:confirm={js::submit(form)} />`.
The handler keeps the parent's refs.

A typed event, when several parents listen or the child decides when it fires:

```rust
#[event]
pub struct RowToggled { pub id: String, pub selected: bool }
// parent: <tbody on:RowToggled={js::count(event, rows)}>
```

```js
import { RowToggled } from "corten/events";
row.dispatchEvent(new RowToggled({ id, selected: true }));
```

List the event in `App::components`. Handlers several components share go in their own module:
`script!(pub mod validity = "validity.js");`.

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

## Live rows (feature `feed`)

```rust
// a Feed<Stock> where Stock: Row (has an id). Routes:
.route(Stocks::PATH, get(|State(m): State<Market>| async move { m.stocks.snapshot() }))
.route(StocksLive::PATH, get(|State(m): State<Market>, since: Since| async move { m.stocks.live(since) }))
// writes: m.stocks.write(|tx| { let s = tx.get_mut(&id)?; s.set_price(cents); Some(()) });

// the component's root: <div on:connect={js::watch(price, feed, signal)}>
```

```js
import { subscribe } from "corten";

/** @param {HTMLTableCellElement[]} cells @param {string} feed @param {AbortSignal} signal */
export function watch(cells, feed, signal) {
  subscribe(feed, {
    snapshot: (rows) => rows.forEach((row) => paint(cells, row)),
    patch: (changes) => { for (const c of changes) if (c.op === "upsert") paint(cells, c.value); },
  }, { signal });
}
```

The server renders the rows first; the feed only keeps them current. Clients resume from their
last patch after a disconnect. Across several instances, use `corten-cluster`.

## Heavy client state (feature `solid`)

For a grid of thousands of live rows, render with Solid from an `on:connect` handler:
`import { h, mount, createStore } from "corten/solid"`, then
`mount(el, () => h(Grid, ...), { signal })` and feed it from `subscribe`. Use it only where the
plain DOM pattern above gets complicated.

## Widgets

Use `corten-ui` (number field, combobox, tabs, menu button) before writing one. For anything
else, use the platform first: `<dialog>`, `popover`, `<details>`, and CSS anchor positioning.
