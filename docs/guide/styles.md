# Styles

[Back to the guides](../../README.md#documentation)

Each component's styles are a CSS file next to it, scoped to it at compile time; design tokens
are one JSON file; both reach the browser as one cached stylesheet. There is no CSS build step.

```css
/* src/views/offer_list.css */
.table { font-variant-numeric: tabular-nums; }
.row:hover { background: var(--color-surface-2); }
.row.selected { outline: 2px solid var(--color-accent); }
```

```rust,ignore
#[island(css)]  // or #[component(css)] for markup without a script
pub fn OfferList<'a>(offers: &'a [Offer], selected: Option<&'a str>) -> impl Render {
    html! {
        <table class={Table}>
            for o in offers { <tr class={(Row, (selected == Some(&o.id)).then_some(Selected))}>...</tr> }
        </table>
    }
}
```

| You write | Corten checks or generates |
|---|---|
| `offer_list.css` next to `OfferList` | every class prefixed, `.offer-list__row`; a rule that does not start at one of the file's classes (or the island's tag) does not compile, so a component cannot style what it does not render |
| `class={Row}`, `class={[Row, Selected]}` | static markup, `class="offer-list__row"`; a class the file lacks, another component's class, or one of the file's classes written as text (`class="row"`) does not compile |
| `class={(Row, cond.then_some(Selected), extra)}` | joined when the page renders; `None` adds nothing, an empty list leaves the attribute out |
| a class nothing uses | `warning: constant Hot is never used` (an error under `-D warnings`) |
| `#[component(css = OfferList)]` | a helper in the same module shares the stylesheet, and its uses count |
| `tokens!("tokens.json")` | custom properties (`--space-4`), theme overrides (`[data-theme="dark"]`, `[data-density="compact"]`), enums (`Space::_4.var()`) and typed responsive utilities: `gap(Responsive::new(Space::_2).md(Space::_4))` prints `gap-2 md:gap-4` |
| `App::components(&[TOKENS, OfferList, ...])` | one `components.css` in the listed order, linked from `<head>` and precached by the PWA; a component rendered but not listed is an error under `corten dev` |

Variants are Rust: `match tone { Tone::Danger => Danger, Tone::Neutral => Neutral }` picks the
class, so adding a variant without a rule does not compile. A component library is a crate that
exports `pub const COMPONENTS: &[Component]`; the app passes it to `App::components`.
`static/style.css` holds what belongs to no component: element defaults, fonts and the shell.
Link it before `{doc.head}` so component rules win ties. Inline `style=` stays blocked by the
CSP; one-off values come from the token utilities instead.
