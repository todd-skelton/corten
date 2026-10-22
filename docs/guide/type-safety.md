# Type safety

[Back to the guides](../../README.md#documentation)

Every value that crosses from Rust to the browser or to an agent is defined once, in Rust, and
checked on the other side.

| Seam | How it is checked |
|---|---|
| Handler to markup | `html!` is Rust: a missing field or prop is a compile error |
| Links to routes | `#[derive(Route)]`: the router, every link, `req.redirect` and `doc.current` use the same struct; a path as text does not compile (a URL outside the app is an explicit `Uri`) |
| Views to islands | `#[island]`: refs and actions are names from the island's own markup; `Types::components` types `refs`, `props` and every action in its script |
| Markup to stylesheet | `#[component(css)]`: `class={Name}` is a constant from the component's own CSS file |
| Tokens to views | `tokens!`: scales are enums and utilities take them, per breakpoint |
| Text to catalogs | `messages!`: every locale has every key with the same arguments |
| Form to handler | `#[derive(Fields)]` names are `FieldName`s, the only key `name=`, `Errors` and corten-ui's fields take; serde parses the post |
| Server to island | `corten::types::Types::new().add::<Row>().components(COMPONENTS).sync(path)` in a test writes TypeScript declarations and fails while they are stale; `npm run check` then fails in every island that reads an old name |
| Server to agent | tool input and output schemas generated from the structs (feature `agent`) |

Untyped props and rows are `unknown`, not `any`. The full table, and what is not checked, is
in [Surfaces](../architecture.md#types-across-seams).

## Generating the island types

A test in the app writes the TypeScript declarations its islands are checked against, and fails
while the committed file is stale:

```rust,ignore
#[test]
fn island_types_match_the_server() {
    corten::types::Types::new()
        .add::<Stock>()
        .components(&[LivePrices])
        .sync(concat!(env!("CARGO_MANIFEST_DIR"), "/types/app.d.ts"));
}
```
