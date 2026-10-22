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
| `let (rows, rest) = refs();`, `ref={rows}` on a `<tbody>` | a parameter `@param {HTMLTableSectionElement} rows` (`HTMLTableSectionElement[]`: every element with the ref) |
| `on:click={js::showAll(..)}` | runs `showAll` on a click on that element |
| `event`, `target`, `signal`, `scope` | the event (`MouseEvent` for a click), the element with the `on:`, an `AbortSignal` that aborts when it leaves the page, the component's root element |
| a Rust value: `&str`, a number, `bool`, a typed route, any `Serialize` (feature `json`) | the JSON value |
| `on:connect={..}` | runs when the element joins the page (first load, navigation, patch, a script's insert) |

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
modules are compiled into the binary, served at versioned URLs, preloaded on the pages that
render them and imported when markup that names them joins the page. List each component once,
`App::new(routes).components(&[OfferList])`.

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
(see [Type safety](type-safety.md)). List the event in `App::components` like a component.

## Shared scripts

Handlers several components use live in a module of their own:
`script!(pub mod validity = "validity.js");` declares `validity::check(..)` and the constant
`validity::COMPONENT` to list.

## State

A handler is a plain function shared by every instance, so state that outlives one event lives
in the DOM (an attribute, an `<output>`'s value) or with an element: `state(scope, () => init)`
from `corten` gives the same object for the same element until it is gone.

## Rules

1. HTML comes from the server, in `html!`. Scripts never build markup from strings; they move,
   show or hide server-rendered nodes. Extra rows ship in a `<template ref={rest}>`.
2. A script reaches the page only through its arguments: refs, `event`, `target`, `signal`,
   `scope`, values. A handler that looks elements up by selector is a smell.
3. User-facing text lives in views or the view model, never in scripts.
4. Cleanup is `signal` alone: pass it to `addEventListener`, `fetch`, `subscribe` and `mount`.
   See [Cancellation](cancellation.md).

## Screens with client state

For screens whose state lives in the browser (a grid of thousands of live rows), a handler can
render part of a component with Solid and keep its rows current from a [feed](live-data.md):

| Piece | Server | Browser |
|---|---|---|
| Live rows | `corten::feed::Feed<Row>`: `write(\|tx\| ...)`, `snapshot()`, `live(since)` (feature `feed`) | `subscribe(url, { snapshot, patch, status }, { signal })` in an `on:connect` handler |
| Rendering | serves Solid as `corten/solid` (feature `solid`) | `mount(el, (server) => h(Grid, ...), { signal })` in the same handler |
