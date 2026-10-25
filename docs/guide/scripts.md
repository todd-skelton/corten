# Scripts

[Back to the guides](../../README.md#documentation)

A component with behavior has a script next to it. The script exports plain functions; the
markup calls them, by name, with the elements and values they use. There is no component
object, no context and no lifecycle class: a handler is a function, and its arguments say
everything it touches.

```rust,ignore
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

| In the markup | In the script |
|---|---|
| `#[component(js)]` on `OfferList` | `offer_list.js` next to it; `js::showAll` is its `export function showAll` |
| `let (rows, rest) = refs();`, `ref={rows}` on a `<tbody>` | a parameter `@param {HTMLTableSectionElement} rows` (`HTMLTableSectionElement[]`: every element with the ref; `Map<string, ..>`: every one by its `key={..}`) |
| `on:click={js::showAll(..)}` | runs `showAll` on a click on that element |
| `row.closest()` | the element with ref `row` around the event's target (the handler runs only inside one) |
| `js::Selection { rows: row, count }` | a `@typedef` of elements: one object argument |
| `event`, `target`, `signal`, `scope` | the event (`MouseEvent` for a click), the element with the `on:`, an `AbortSignal` that aborts when it leaves the page, the component's root element |
| a Rust value: `&str`, a number, `bool`, a typed route, any `Serialize` (feature `json`) | the JSON value (an integer past 2^53 stops debug builds, since a JavaScript number cannot hold it exactly: pass a 64-bit id as a string) |
| `on:connect={..}` | runs when the element joins the page (first load, navigation, patch, a script's insert); on the first load it runs before the page's `load`, and a navigation it starts then (a link it clicks, a form it submits) is a full page load, not a swap |

**What the compiler checks.** `#[component(js)]` reads the script when the crate compiles and
gives the component a module `js` with one Rust function per export, its parameters typed by
the JSDoc. A ref's element type is inferred from the tag it is put on, `event`'s from the
`on:` name and `target`'s from the tag, so passing a `<tbody>` ref where the handler takes a
`<button>`, a `MouseEvent` where it takes a `KeyboardEvent`, a number where it takes a
`string`, or calling a handler the script does not export is a compile error that points at
the call. Inside the script, `tsc --checkJs` checks the function bodies against the same JSDoc.
A ref on several kinds of element is typed as one they all are: `ref={tab as HTMLElement}`.

**What the browser gets.** Plain HTML: `data-ref="rows"` and
`data-on="click:offer_list.showAll(@rows,@rest,$t)"`. One listener per event type on the
document runs the handlers, innermost element first, and `event.stopPropagation()` stops the
walk as it would stop bubbling. Nothing runs per element until an event reaches it. Script
modules are compiled into the binary, served at versioned URLs, loaded as module scripts by the
pages that render them and imported when markup that names them joins later. List each component once,
`App::new(routes).components(&[OfferList])`.

## Server or browser

Do the work where it is fastest for the user. A round trip costs a network hop and a render
(tens of milliseconds on a phone, more on a poor connection); a handler changing the page costs
microseconds, and a [view](#views) changes 1,000 rows in a few milliseconds. So the server
answers what only it can, and the browser does the rest.

| The page needs | Do it with | Why |
|---|---|---|
| to sort, filter, select or page through rows it already has | a handler, and a `#[component(view)]` for many rows | no request |
| to show, hide or expand what it already has | `<details>`, `popover`, `<dialog>`, or a handler with a `<template>` | the platform does it |
| a value computed from what is on the page (a total, a preview) | a handler | no request |
| to save, buy, sign in or change anything | a form the server answers, `data-patch` | the server decides what a write does |
| data it does not have (the next results, a search across everything) | a link, or a `GET` form | only the server has it |
| data that changes on the server | a [feed](live-data.md) or `<corten-live>` | sent when it changes |

A handler that makes a server action instant keeps the action: rows a handler filters as the
user types sit in a `GET` form that filters them on the server without JavaScript.

## Components and their roots

A component with refs, or whose handlers take `scope`, has one root element. The root is
where its refs end: `data-scope="offer-list"` marks it, and a ref resolves to the elements with
that name inside the root and outside any nested component's root. Two `OfferList`s on one
page never see each other's rows. The root can be any element, so a component can be a `<tr>`
or an `<li>`.

## Handlers as props

A child component takes a handler the parent decides, as a prop of type `Handler`, and puts it
on one of its elements. The parent sets it with `on:name`:

```rust,ignore
#[component]
fn ConfirmButton<'a>(label: &'a str, on_confirm: Handler<'a>) -> impl Render {
    html! { <button type="button" on:click={on_confirm}>{label}</button> }
}

#[component(js)]
fn CartLine<'a>(item: &'a Item) -> impl Render {
    let form = refs();
    html! {
        <li>
            <form ref={form} method="post" action={RemoveItem { id: &item.id }} data-patch>
                <ConfirmButton label="Remove" on:confirm={js::submit(form)} />
            </form>
        </li>
    }
}
```

The handler keeps the parent's refs and scope wherever the child renders it:
`data-on="click:cart_line.submit(@cart-line/form)"`. `None::<Handler>` leaves the `on:` out.

## Typed events

A component tells its parents something happened by dispatching an event. `#[event]` declares
one from a Rust struct; the browser imports its class from `corten/events`, and markup listens
with `on:` and the type's name:

```rust,ignore
#[event]
pub struct RowToggled {
    pub id: String,
    pub selected: bool,
}

// a parent: <tbody on:RowToggled={js::count(event, rows)}>
```

```js
import { RowToggled } from "corten/events";
row.dispatchEvent(new RowToggled({ id, selected: true }));
```

The class bubbles and carries the fields; `Types::components` declares it with the fields' types
(see [Type safety](type-safety.md)). List the event in `App::components` like a component. A
handler parameter typed with a class imported from `"corten/events"` takes only the `event` of
an `on:` for that type, a compile error otherwise; the `#[event]` type is in scope in the
component's module.

## Lists and larger components

A grid of 500 rows with a handler on every checkbox prints 500 handler calls. Four tools keep a
large component small and its handlers short.

**One handler for the list.** `row.closest()` is the element with ref `row` that holds the
event's target, like the DOM's `event.target.closest(..)`. Put the handler on the container; it
runs only for events inside such an element:

```rust,ignore
<tbody on:click={js::toggle(event, select.closest(), sel)}>
    for line in lines {
        <tr ref={row}><td><input ref={select} type="checkbox" value={line.id}></td>..</tr>
    }
</tbody>
```

The rows need no handler of their own, so a row component can be markup only.

**Groups.** When several handlers need the same elements, describe them once in the script with
a `@typedef`. Each one with an element among its properties becomes a struct in `js`, checked
field by field, built once and passed as one argument:

```js
/**
 * @typedef {object} Selection
 * @property {HTMLTableRowElement[]} rows
 * @property {HTMLInputElement[]} boxes
 * @property {HTMLOutputElement} count
 */

/** @param {MouseEvent} event @param {HTMLInputElement} box @param {Selection} sel */
export function toggle(event, box, sel) { .. }
```

```rust,ignore
let sel = js::Selection { rows: row, boxes: select, count };
// on:click={js::toggle(event, select.closest(), sel)}, on:change={js::selectAll(sel)}, ..
```

The page gets `{rows:*row,boxes:*select,count:@count}`; the script gets an object.

**Keys.** `key={item.id}` on the elements of a ref (`data-key` in the page) lets a parameter
typed `Map<string, HTMLTableCellElement>` get them by key, so rows from a [feed](live-data.md)
find their cells without reading attributes:

```rust,ignore
<td ref={qty} key={item.id}>{line.qty}</td>
// on:connect={js::live(qty, url::Levels, signal)}, live(cells, feed, signal) with cells.get(level.id)
```

**Values the markup owns.** `html!` owns what it captures, so a value borrowed from one of them
is computed inside the markup, with `let`:

```rust,ignore
let saved = cents.map(money);
html! {
    let shown = saved.as_deref().unwrap_or(typed);
    <input value={shown}>
}
```

A patch removes rows as well as replacing them: `req.patch(html! { for id in &done { {Remove(RowId(id))} } })`
(`corten::view::Remove`, a `<template id data-remove>` the runtime applies). `RowId` is the
`#[derive(Id)]` the row prints its id with ([typed ids](views.md#typed-links-and-messages)), so
the two cannot drift.

## Shared scripts

Handlers several components use live in a module of their own:
`script!(pub mod validity = "validity.js");` declares `validity::check(..)` and the constant
`validity::COMPONENT` to list.

## Libraries

A crate of components for other apps prefixes the names it puts in the page, so they never meet
an app's own: `[package.metadata.corten] scripts = "ui"` in its `Cargo.toml` makes its scripts
`ui.combo_box.js` and its roots `data-scope="ui-combo-box"`. `corten-ui` does this.

## When handlers run

A handler runs while its event is dispatched, so it can call `event.preventDefault()`. The
first page loads the modules its handlers name as module scripts of its own, right after the
runtime; Corten ends each component's module with lines that hand its exports to the runtime
(a `corten:module` event the runtime takes only from the module the page's own script tag for
that name loaded, and only while the page loads; it is not an API, and `"corten"` exports nothing for it), so its
handlers are in when the browser runs the page's scripts, before `DOMContentLoaded`, with
no `import()` to wait for. Markup the runtime inserts (a navigation, a patch, a live fragment)
waits for the modules its handlers name, and modules named inside a `<template>` load with the
page. On the first page, an event that arrives before its module is in gets the browser's
default and no handler, as without JavaScript. `on:connect` waits for its module instead.

## State

A handler is a plain function shared by every instance, so state that outlives one event lives
in the DOM (an attribute, an `<output>`'s value) or with an element: `state(scope, () => init)`
from `corten` gives the same object for the same element until it is gone.

## Rules

1. Markup is written in `html!`, never built from strings in a script. A script makes new
   elements by cloning markup the server rendered (a `<template ref={rest}>`, or a view's rows,
   `rows(..).push(items)`), and moves, shows, hides or writes the ones on the page.
2. A script reaches the page only through its arguments: refs, `event`, `target`, `signal`,
   `scope`, values. A handler that looks elements up by selector is a smell.
3. User-facing text lives in views or the view model, never in scripts. Text that depends on
   what a script computed has no typed way in yet
   ([#17 Scripts have no typed way to show user-facing text they compute](https://github.com/todd-skelton/corten/issues/17)).
4. Cleanup is `signal` alone: pass it to `addEventListener`, `fetch`, `subscribe` and `mount`.
   See [Cancellation](cancellation.md).

## Views

A component a script changes often (a row of a grid with thousands) can be a
`#[component(view)]`. Its handlers then write the component's holes by name instead of finding
elements:

```rust,ignore
#[component(view)]
pub fn Row<'a>(id: u32, label: &'a str, selected: bool, pick: Ref<el::HTMLAnchorElement>) -> impl Render {
    html! {
        <tr key={id} data-selected={selected}>
            <td>{id}</td>
            <td><a ref={pick}>{label}</a></td>
        </tr>
    }
}

#[component(js)]
fn Grid<'a>(items: &'a [Item]) -> impl Render {
    let (body, template, pick) = refs();
    html! {
        <table>
            <tbody ref={body} on:click={js::select(pick.closest(), body)}>
                for item in items {
                    <Row id={item.id} label={&item.label} selected={false} pick={pick} />
                }
            </tbody>
            <template ref={template}><Row id={0} label="" selected={false} pick={pick} /></template>
        </table>
    }
}
```

```js
// grid.js
import { Row, rows } from "corten/views";
/** @import { Holds } from "corten/views" */

/** @param {HTMLAnchorElement} link @param {Holds<Row, HTMLTableSectionElement>} body */
export function select(link, body) {
  const list = rows(body, Row); // body's rows, the same list every time
  const row = list.of(link); // the row that was clicked
  if (row) row.selected = true;
  list.at(5).label += " !!!"; // writes that one text node
}
```

Views are not a second way to handle events: the markup calls handlers with `on:` as everywhere
else, and a handler that gets the list's parent (and the `<template>`, to make new rows) calls
`rows(body, Row, template)`:

| `rows()` | |
|---|---|
| `at(i)`, `get(key)`, `of(event.target)`, `indexOf(view)`, `length`, iteration | the views of the rows |
| `push(items)`, `remove(view)`, `swap(i, j)`, `clear()` | change the rows; new ones are clones of the template's row, filled with their data |
| `replace(items)` | shows exactly `items`: rows with a key it had keep their element and are filled again |

One parent has one list, whichever handler asks, and it notices rows changed without it (a
server patch, a sort) the next time it is used.

The parameter's type says whose rows it holds: `Holds<Row, HTMLTableSectionElement>` (or
`Holds<Row, HTMLTemplateElement>` for the template). `tsc` then rejects `rows(body, Card)` and
`rows(plainElement, Row)`, and the Rust compiler rejects passing a ref whose element the markup
does not fill with `Row`s only: the ref must be on the element whose children are a loop of
`<Row .. />` (or on a `<template>` holding one `<Row .. />`), with nothing else beside them. A
component that takes such a ref as a prop names its type
`Ref<el::HTMLTableSectionElement, RowsOf<{ view_id("Row") }>>`.

| In the markup | In the view |
|---|---|
| `{label}`, the only child of an element | `label`: reads and writes that element's text |
| `data-selected={selected}`, an attribute's whole value | `selected`: the attribute (a `bool` adds or removes it) |
| `value={qty}` or `checked={on}` on an `<input>` | `qty`, `on`: what the input shows now (the user's edit), and its default |
| `key={id}` on the root | `Row.key(el)` and `Row.keyOf(data)`, what `rows()` keys by; when `id` is also used outside holes (a child's prop, an element id), only `Row.key(el)`: the server makes the rows, and `rows()` has no `push` or `replace` |
| a parameter used any other way (`"Label: "{label}`, inside a loop, in a `class`), or a `Ref` | nothing: a script could not write it alone |

Holes are found when the crate compiles, on elements whose place is fixed (no loop, condition,
expression or component before them), so the view walks straight to them. Numbers, text and
`bool` parameters can be properties. A number property is a JavaScript number in the page, so
debug builds stop a value past 2^53 (a 64-bit id), as they do for a handler's number argument:
make such a value a string. `corten::types` declares the classes and `rows()` in
`corten/views`, so `row.lable = "x"` or `row.label = 3` fails `tsc`. A view needs one root
element and at least one hole, or the component does not compile. List the component in
`App::components`; `rows()` and the classes are served as one module, mapped as
`corten/views`, and only to apps with views.

## Live rows

Rows the server keeps current come from a [feed](live-data.md), and the handler writes them
through [views](#views):

| Piece | Server | Browser |
|---|---|---|
| Live rows | `corten::feed::Feed<Row>`: `write(\|tx\| ...)`, `snapshot()`, `live(since)` (feature `feed`) | `subscribe(url, { snapshot, patch, status }, { signal })` in an `on:connect` handler |
| Rendering | the rows, rendered with `html!` as `#[component(view)]`s | `rows(body, Row).get(row.id).price = row.price` for each changed row, in the same handler |
