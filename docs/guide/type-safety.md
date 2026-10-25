# Type safety

[Back to the guides](../../README.md#documentation)

Every value that crosses from Rust to the browser or to an agent is defined once, in Rust, and
checked on the other side.

| Seam | How it is checked |
|---|---|
| Handler to markup | `html!` is Rust: a missing field or prop is a compile error |
| Links to routes | `#[derive(Route)]`: the router, every link, `req.redirect` and `doc.current` use the same struct; a path as text does not compile (a URL outside the app is an explicit `Uri`) |
| Markup to handlers | `#[component(js)]` reads the script's JSDoc when the crate compiles: `js::name` exists only for an export, and each argument (a ref, typed by the tag it is on; `event`, `target`, `signal`, `scope`; a value) must match its `@param` type |
| Typed events | `#[event]` structs: `on:Name` must name one, and `Types::components` declares its class in `corten/events` with the fields' types |
| Markup to stylesheet | `#[component(css)]`: `class={Name}` is a constant from the component's own CSS file |
| Tokens to views | `tokens!`: scales are enums and utilities take them, per breakpoint |
| Text to catalogs | `messages!`: every locale has every key with the same arguments |
| Form to handler | `#[derive(Fields)]` names are `FieldName`s, the only key `name=`, `Errors` and corten-ui's fields take; serde parses the post |
| Handler to JSON client | `Json<T>` takes a `ToJson`; `#[derive(ToJson)]` reads `#[serde(...)]` and refuses one it cannot write as serde does (`flatten`, `serialize_with`), so what it writes is what serde reads back |
| Another service's JSON to a page | `corten::json::from_slice` takes a `FromJson`; `#[derive(FromJson)]` reads `#[serde(...)]` and refuses one it cannot read as serde does (`flatten`, `deserialize_with`), and needs `Deserialize` too, which reads whatever its reader gives up on |
| Server to script | `corten::types::Types::new().add::<Row>().components(COMPONENTS).sync(path)` in a test writes TypeScript declarations and fails while they are stale; `npm run check` then fails in every script that reads an old name |
| Server to agent | tool input and output schemas generated from the structs (feature `agent`) |

Untyped rows are `unknown`, not `any`. The full table, and what is not checked, is
in [Surfaces](../architecture.md#types-across-seams).

## Checking handler calls

Nothing is generated between markup and script: the call is checked against the script itself.

```js
// offer_list.js
/** @param {HTMLTableSectionElement} rows @param {HTMLTemplateElement} rest @param {HTMLButtonElement} button */
export function showAll(rows, rest, button) { /* .. */ }
```

```rust,ignore
let (rows, rest) = refs();
// <tbody ref={rows}>, <template ref={rest}>
<button on:click={js::showAll(rows, rest, target)}>     // compiles
<button on:click={js::showAll(rest, rows, target)}>     // error: the handler takes an HTMLTableSectionElement, and this is an HTMLTemplateElement
<button on:click={js::show_all(rows, rest, target)}>    // error: no function show_all in js
```

Every parameter needs a JSDoc type; an untyped one is a compile error that names it.

## Generating the script types

A test in the app writes the TypeScript declarations its scripts read (feed rows, typed
events), and fails while the committed file is stale:

```rust,ignore
#[test]
fn script_types_match_the_server() {
    corten::types::Types::new()
        .add::<Stock>()
        .components(views::COMPONENTS)
        .sync(concat!(env!("CARGO_MANIFEST_DIR"), "/types/app.d.ts"));
}
```
