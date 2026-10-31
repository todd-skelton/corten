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
4. A helper that shares the file: `#[component(css = OfferList)]` in the owner's module,
   `#[component(css = super::offers::OfferList)]` from another (one stylesheet for a screen whose
   panels are in several files). Classes are in scope only in components; never write a plain
   `fn` that returns one of them (`Ok` would be `Result::Ok` there), and never `use` the
   hidden `__corten_css_*` module.
5. Design tokens: W3C Design Tokens files (DTCG 2025.10). `corten::style::tokens!("tokens/tokens.resolver.json")`
   gives custom properties, each theme context's changes (`[data-theme="dark"]`) and typed
   scales; breakpoints and utilities go under `$extensions."run.corten"`. Put `TOKENS` first in
   `App::components`.
   A `var(--x)` without a fallback that no stylesheet on the same pages declares fails at
   startup, naming the file; a property a script sets takes a fallback, `var(--x, 0)`.
6. `static/style.css` holds only element defaults, fonts and the layout. It is linked
   before `<Corten />` in the layout.
7. Inline `style=` is blocked by the CSP. Use a class or a token utility instead: a fixed value is
   its own function (`mx_auto()`, `gap_3()`); the utility's function (`gap(space)`,
   `gap(Responsive::new(Space::_2).md(Space::_4))`) is for a value chosen at run time or per
   breakpoint. A size drawn from data (a meter, a chart) is a custom property: register it in the
   component's CSS (`@property --fill { syntax: "<percentage>"; inherits: false; initial-value: 0% }`,
   read with `var(--fill)`) and set it with `style={FILL.pct(used)}`; no policy change is needed.
8. Every component's CSS ships in one `components.css`. Only when a part of the app most
   visitors never open (an admin area) adds CSS worth measuring, make it an area and list its
   components there, `.area(|area| admin::area(area).components(&[..]))`: their
   stylesheet is linked from pages under that prefix only. A component used in several areas
   is listed in each (they share a file of its rules); one most pages use stays in
   `App::components`, and so do tokens. Details: `docs/guide/styles.md`, "Areas".
