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
5. A component with refs, whose handlers take `scope`, or whose script imports `keep`, has one
   root element; refs resolve inside it and outside nested components. Any element can be the root, a `<tr>` included.
6. A ref on several kinds of element: `ref={tab as HTMLElement}`.
7. State that outlives one event lives in the DOM or in `keep(() => init)` from `corten`: the
   running handler's component's object, one per instance (a second call with another `init`
   gets the first one), with no element passed, so a handler takes no `scope` just for its state.
   Take it first, before any `await`. Only a component's own script imports it, not a `script!`
   module. Handlers are shared by every instance, so never keep state in
   module variables.
8. A control that does nothing without the script renders `hidden`; an `on:connect` handler
   shows it.
9. List the component in `App::components`.
10. A value the script computes that the server also draws is a second copy: draw it on the server
    only when a round trip is fast enough; otherwise keep the arithmetic in a `script!` module
    that imports nothing and test both sides against one table of cases (scripts guide, A value
    both sides compute).

## Lists and large components

1. One handler for a list: put it on the container with `row.closest()`, the element with ref
   `row` around the event's target: `<tbody on:click={js::toggle(event, select.closest(), sel)}>`.
   Rows then need no handlers (a row component can be plain `#[component]`).
2. Elements (or messages) several handlers need: a `@typedef {object} Selection` with `@property` lines in the
   script becomes `js::Selection { rows: row, count }`; build it once, pass it as one argument.
3. Rows a feed updates by id: `key={item.id}` on the cells, a `Map<string, HTMLTableCellElement>`
   parameter, `cells.get(row.id)`. Do not read `data-*` attributes for this.
4. A value borrowed from one the view owns: compute it inside `html!` with `let`. A view lent
   to a prop (`action={&html! { .. }}`) borrows what it uses, so read the same local in it and
   in the rest of the markup; do not rebind it with `as_ref()` first.
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
`@param {NumberFormat}`, `{MoneyFormat}`, `{DateFormat}` or `{PercentFormat}` (`t.js().percent(2)` or `t.js().change(2)`, from `"corten"`); call `money(cents)`
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

The event is served with the component's script that imports it, so it is not listed (only one
that a `static/` module alone imports is). Handlers several components share go in their own module:
`script!(pub mod validity = "validity.js");`, listed as `validity::COMPONENT`. A page whose
handlers call an unlisted module fails in a debug build (500 naming the module): list it, and
cover each page with a router test so the check runs.

## A fragment the server keeps current

No script needed. Stream components with an `id`; the runtime swaps each one in, like a patch:

```rust
// in the page: the trade count it was drawn at, so a current page is not sent it again
<corten-live src={Trades} since={n}><LastTrade text={&last} /></corten-live>

// GET /live/trades: one value in a tokio watch channel, (trade count, text)
async fn trades(State(m): State<Market>, since: LiveSince) -> LiveResponse {
    corten::live_watch(m.trades.subscribe(), since, |(n, _)| *n, |(_, text)| LastTrade { text }.to_html())
}
```

Another source (a change stream, a queue) is `corten::live(stream)`: give each item its version,
`Fragment::new(html).cursor(v)`, and use the `LiveSince` argument (the page's `since`, or
`Last-Event-ID` after a reconnect) to send nothing until the next change when it is current. A
slow part sent once is a stream of one item. See the live-data guide.

A job another process runs has only a read to repeat. Many pages following one job: an
`App::task` reads it on an interval into a `watch` channel, and each stream is `live_watch`. One
page's job: `corten::live` over an `unfold` loop that reads, sends only when the version moved, ends
after the settled state and, after a few failed reads, draws that it could not read before it
ends (the live-data guide has it). `App::task` starts with the app: jobs that start later share
one task that sends to a `watch` channel per job.

The element shows its connection as `status` (`connecting`, `live`, `reconnecting`, `closed`,
`ended`) and `stale-since` (when a live stream dropped): style a stale part with
`corten-live[status=reconnecting]`, not with a script that watches for patches. A script that
must react (writing "Stale since 10:42") takes `on:corten-status={js::stale(target, ..)}` on the
element (a bubbling event on each change) and reads the attributes; never a `MutationObserver`.

## Live rows (feature `feed`)

```rust
// a Feed<Stock> where Stock: Row (has an id). In the routes module, each feed route names its rows:
//   #[route("/feed/stocks", get = stocks, rows = Stock)] pub struct Stocks;
//   #[route("/feed/stocks/live", get = stocks_live, rows = Stock)] pub struct StocksLive;
async fn stocks(State(m): State<Market>) -> FeedResponse<Stock> { m.stocks.snapshot() }
async fn stocks_live(State(m): State<Market>, since: Since) -> FeedResponse<Stock> { m.stocks.live(since) }
// writes: m.stocks.write(|tx| { let s = tx.get_mut(&id)?; s.set_price(cents); Some(()) });

// the component's root: <div on:connect={js::watch(price, url::Stocks, signal)}>
```

```js
import { subscribe } from "corten";
/** @import { FeedUrl } from "corten" */
/** @typedef {import("../types/app").Stock} Stock */

// FeedUrl<Stock> takes only a route with rows = Stock (checked when the crate compiles);
// subscribe() takes the row type from it, so rows and changes are Stock without a @param.
/** @param {HTMLTableCellElement[]} cells @param {FeedUrl<Stock>} feed @param {AbortSignal} signal */
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

Use `corten-ui` (number field, combobox, tabs, menu button, toaster, copy button, checkbox group, required-when) before writing
one. One choice that shows and hides parts in several places of the page (a period both a figure and a
list below a chart follow) is `ChoiceGroup` with a `ShownFor` around each part, both given one `Chosen { id, value }`, never CSS that repeats
the choice's values or a script that finds the parts. For anything else, use the platform first: `<dialog>`, `popover`, `<details>`, and CSS
anchor positioning. A `<dialog>` the server can draw `open` (a form it refuses) gets
`on:connect={corten_ui::dialog::modal(target)}`, so it is modal, not just open; one the address
opens and a link closes (`?selected=..`) gets `dialog::modalClosedBy(target, close, signal)` with
the link as the `close` ref, so a close request (Escape) goes to its address. Do not write these
handlers in the app.
