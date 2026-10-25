# Styles

A component's styles are `snake_name.css` next to it. Every rule starts at one of the file's
classes; Corten prefixes them (`.offer-list__row`) and generates a constant for each.

```css
/* src/views/offer_list.css */
.table { font-variant-numeric: tabular-nums; }
.row:hover { background: var(--color-surface-2); }
.row.selected { outline: 2px solid var(--color-accent); }
```

```rust
#[component(css)]   // #[component(css, js)] when it has a script too
pub fn OfferList<'a>(offers: &'a [Offer], selected: Option<&'a str>) -> impl Render {
    html! {
        <table class={Table}>
            for o in offers { <tr class={(Row, (selected == Some(&o.id)).then_some(Selected))}>...</tr> }
        </table>
    }
}
```

1. `class={Row}` or `class={[Row, Selected]}` is static. `class={(Row, cond.then_some(X))}` is
   decided at render time.
2. A class that is missing from the file, belongs to another component, or is written as a
   string does not compile. An unused class is a warning, which is an error under
   `corten check` (clippy runs with `-D warnings`).
3. Variants are a Rust `match` from an enum to class constants.
4. A helper in the same module that shares the file: `#[component(css = OfferList)]`.
5. Design tokens: `corten::style::tokens!("tokens.json")` gives custom properties, themes
   (`[data-theme="dark"]`) and typed scales. Put `TOKENS` first in `App::components`.
6. `static/style.css` holds only element defaults, fonts and the layout. It is linked
   before `<Corten />` in the layout.
7. Inline `style=` is blocked by the CSP. Use a class or a token utility instead.
8. Every component's CSS ships in one `components.css`. Only when a part of the app most
   visitors never open (an admin area) adds CSS worth measuring, make it an area and list its
   components there, `.area::<Admin>(|admin| admin.route::<..>(..).components(&[..]))`: their
   stylesheet is linked from pages under that prefix only. A component used in several areas
   stays in `App::components`, and so do tokens. Details: `docs/guide/styles.md`, "Areas".
