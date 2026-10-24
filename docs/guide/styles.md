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
#[component(css)]  // #[component(css, js)] with a script too
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
| `offer_list.css` next to `OfferList` | every class prefixed, `.offer-list__row`; a rule that does not start at one of the file's classes does not compile, so a component cannot style what it does not render |
| `class={Row}`, `class={[Row, Selected]}` | static markup, `class="offer-list__row"`; a class the file lacks, another component's class, or one of the file's classes written as text (`class="row"`) does not compile |
| `class={(Row, cond.then_some(Selected), extra)}` | joined when the page renders; `None` adds nothing, an empty list leaves the attribute out |
| a class nothing uses | `warning: constant Hot is never used` (an error under `-D warnings`) |
| `#[component(css = OfferList)]` | a helper in the same module shares the stylesheet, and its uses count |
| `tokens!("tokens.json")` | custom properties (`--space-4`), theme overrides (`[data-theme="dark"]`, `[data-density="compact"]`), enums (`Space::_4.var()`) and typed responsive utilities: `gap(Responsive::new(Space::_2).md(Space::_4))` prints `gap-2 md:gap-4` |
| `App::components(&[TOKENS, OfferList, ...])` | one `components.css` in the listed order, linked from `<head>` and precached by the PWA; a component rendered but not listed is an error under `corten dev` |
| `App::section("/admin", &[SalesChart, ...])` | those stylesheets in a file of their own, `sections/admin.css`, linked only from pages at and under `/admin`; a component listed in two places, or tokens in a section, is a startup error, and a page outside the section that renders one is an error under `corten dev` |

Variants are Rust: `match tone { Tone::Danger => Danger, Tone::Neutral => Neutral }` picks the
class, so adding a variant without a rule does not compile. A component library is a crate that
exports `pub const COMPONENTS: &[Component]`; the app passes it to `App::components`.

A theme can also follow the system: its `"@media"` gives a media condition and the selector its
values apply to under it, so they are written once.

```json
"themes": {
  "[data-theme=\"dark\"]": {
    "@media": { "(prefers-color-scheme: dark)": ":root:not([data-theme=\"light\"])" },
    "color": { "ink": "#ecebf2", "bg": "#15141b" }
  }
}
```

The page is dark when it says `data-theme="dark"`, or when the system is dark and the page did
not choose `light`. `color-scheme` (which themes form controls and scrollbars) is a property,
not a token, so it goes in `static/style.css` next to the same selectors.

`static/style.css` holds what belongs to no component: element defaults, fonts and the shell.
Link it before `{doc.head}` so component rules win ties. Inline `style=` stays blocked by the
CSP; one-off values come from the token utilities instead.

## Sections

Every page links `components.css`, so a large part of the app that most visitors never open (an
admin console, seller tools) adds its CSS to every first load. `App::section` gives that part a
stylesheet of its own:

```rust,ignore
App::new(routes)
    .components(&[TOKENS, Button, OfferList])           // every page
    .section(Admin::PATH, &[SalesChart, AuditTable])    // pages at /admin and under it
```

- The server picks a page's stylesheets by its path, from links built at startup: no per-render
  tracking. A page under nested sections (`/admin`, `/admin/reports`) links each, outermost first.
- A client navigation into the section adds the stylesheet to `<head>` and waits for it before
  swapping the page in, so the page never shows unstyled; if it fails to load, the runtime loads
  the page in full. The stylesheet then stays: its rules match only its components' classes.
- A component that pages in several sections render belongs in `App::components`, and so do
  tokens, which style every page. A patch is styled by the page it lands on and is checked
  against its own URL, so a form whose action sits outside the page's section should answer with
  components from `App::components`.

Splitting has a price: two files compress worse than one, and entering the section costs a
request. Measured on the item page (release build, brotli at its file level): one
`components.css` is 3,223 bytes, 1,047 brotli. With the item page's four components in an
`/items` section, every page gets 593 bytes brotli and item pages 548 more, so the home page
saves 454 bytes and an item page downloads 94 more, in two requests. Leave everything in
`App::components` until a section's share is large enough to measure on the pages outside it.

Without sections a request costs what it did before (callgrind, the item page in process: the
same instruction counts within run-to-run noise). Writing a section's links costs about 120
instructions a page; the item page's navigation as a whole measured 88,341-88,589 instructions
with a section against 87,249-87,331 without; most of the rest of the difference is in serde_json,
which the section code does not call, and the allocator. Sections add 126 bytes gzip to the runtime.
