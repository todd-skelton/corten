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
   `corten`, one object per element (a second call with another `init` gets the first one).
   Handlers are shared by every instance, so never keep state in module variables.
8. A control that does nothing without the script renders `hidden`; an `on:connect` handler
   shows it.
9. List the component in `App::components`.

## Lists and large components

1. One handler for a list: put it on the container with `row.closest()`, the element with ref
   `row` around the event's target: `<tbody on:click={js::toggle(event, select.closest(), sel)}>`.
   Rows then need no handlers (a row component can be plain `#[component]`).
2. Elements (or messages) several handlers need: a `@typedef {object} Selection` with `@property` lines in the
   script becomes `js::Selection { rows: row, count }`; build it once, pass it as one argument.
3. Rows a feed updates by id: `key={item.id}` on the cells, a `Map<string, HTMLTableCellElement>`
   parameter, `cells.get(row.id)`. Do not read `data-*` attributes for this.
4. A value borrowed from one the view owns: compute it inside `html!` with `let`.
5. Rows a write removes: `req.patch(html! { for id in &done { {Remove(RowId(id))} } })`
   (`corten::view::Remove`), with `#[derive(Id)] struct RowId<'a>(&'a str);` and
   `<tr id={RowId(&item.id)}>` on the row. Rows Load more adds: `view::Append(RowsId, rows)`,
   only the new page, never the whole list again.
6. Filters applied on change: a GET form with `data-keep-place` and a handler that calls
   `form.requestSubmit()`; the runtime keeps the scroll position, a
   scrolling rail's scroll, the groups (`<details>`) the user opened and focus. Never save and
   restore them yourself.

## Text a script computes

A count or total the script works out is still a catalog message: pass `t.js().rows_selected()`
(the message unfilled, in the page's locale) and take it as `@param {Message<"count">} label`
(`/** @import { Message } from "corten" */`); show `label({ count })` with `textContent`. A
message with other arguments does not compile at the call. Never write the text in the script,
and do not render every variant in a `<template>` for the script to copy.

A number, amount or date the script works out is written with the page's format, never
`Intl` or `toFixed`: pass `t.js().number()`, `t.js().money(USD)` or `t.js().date(DateStyle::Medium)`
(`.time(..)`, `.date_time(..)`, `.zone(Offset::hours(-5))`) and take it as
`@param {NumberFormat}`, `{MoneyFormat}`, `{DateFormat}` or `{PercentFormat}` (`t.js().percent(2)`, from `"corten"`); call `money(cents)`
or `when(Date.now())`. It writes what the server's `t.money` and `t.date` write. A zone with
daylight saving time stays on the server.

## A child that reports to its parent

A handler prop: the child takes `on_confirm: Handler<'a>` and puts it on an element with
`on:click={on_confirm}`; the parent writes `<ConfirmButton on:confirm={js::submit(form)} />`.
The handler keeps the parent's refs.

A typed event, when several parents listen or the child decides when it fires (a handler
parameter typed with the imported class only takes that event; keep the `#[event]` type in scope
in the component's module):

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
`script!(pub mod validity = "validity.js");`, listed as `validity::COMPONENT`. A page whose
handlers call an unlisted module fails in a debug build (500 naming the module): list it, and
cover each page with a router test so the check runs.

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

When the data has a version, resume instead of redrawing: `Fragment::new(html).cursor(v)` for
each item, `<corten-live src={Trades} since={v}>` in the page, and a `LiveSince(since)`
argument that says where the browser is (the page's `since`, or `Last-Event-ID` after a
reconnect). Send nothing until the next change when it is current. See the live-data guide.

The element shows its connection as `status` (`connecting`, `live`, `reconnecting`, `closed`,
`ended`) and `stale-since` (when a live stream dropped): style a stale part with
`corten-live[status=reconnecting]`, not with a script that watches for patches. A script that
must react (writing "Stale since 10:42") takes `on:corten-status={js::stale(target, ..)}` on the
element (a bubbling event on each change) and reads the attributes; never a `MutationObserver`.

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

## Many rows: views

A row component a script changes often is a `#[component(view)]`: each parameter that is an
element's whole text, an attribute's whole value, or an input's `value`/`checked` becomes a typed
property of a class in `corten/views`. The handler takes the rows' parent as
`Holds<Row, ..>` and calls `rows(body, Row)`; never query rows by hand.

```rust
#[component(view)]
pub fn Row<'a>(id: u32, label: &'a str, selected: bool, pick: Ref<el::HTMLAnchorElement>) -> impl Render {
    html! { <tr key={id} data-selected={selected}><td><a ref={pick}>{label}</a></td></tr> }
}
// the grid: <tbody ref={body} on:click={js::select(pick.closest(), body)}> for item in items { <Row .. /> } </tbody>
// the ref must be on an element whose children are only <Row .. /> (the compiler checks it)
```

```js
import { Row, rows } from "corten/views";
/** @import { Holds } from "corten/views" */

/** @param {HTMLAnchorElement} link @param {Holds<Row, HTMLTableSectionElement>} body */
export function select(link, body) {
  const row = rows(body, Row).of(link);
  if (row) row.selected = true; // writes the attribute, nothing else
}
```

`rows()` has `at`, `get(key)`, `of`, `indexOf`, `push`, `remove`, `swap`, `clear` and keyed
`replace` (pass the `<template>` holding one `<Row .. />` to make new rows). List the view in
`App::components`. Details: `docs/guide/scripts.md`, "Views".

## Widgets

Use `corten-ui` (number field, combobox, tabs, menu button, toaster) before writing one. For anything
else, use the platform first: `<dialog>`, `popover`, `<details>`, and CSS anchor positioning. A
`<dialog>` the server can draw `open` (a form it refuses) gets
`on:connect={corten_ui::dialog::modal(target)}`, so it is modal, not just open; do not write that
handler in the app.
