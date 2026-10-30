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
| `.card` in `Card`'s own stylesheet | `class={Card}` in its markup is the class, `card__card`: name the root class after the component |
| `class={(Row, cond.then_some(Selected), extra)}` | joined when the page renders; `None` adds nothing, an empty list leaves the attribute out |
| a class nothing uses | `warning: constant Hot is never used` (an error under `-D warnings`) |
| `#[component(css = OfferList)]`, `css = super::offers::OfferList` | a helper component shares the owner's stylesheet from any module that can see the owner (a screen's panels in files of their own), and its uses count; classes are in scope only in a component, so a plain `fn` that picks a class is a helper component or a `match` in the markup |
| `tokens!("tokens.json")` | custom properties (`--space-4`), theme overrides (`[data-theme="dark"]`, `[data-density="compact"]`), enums (`Space::_4.var()`) and typed utilities, a function per value (`gap_2()` prints `gap-2`, `mx_auto()` `mx-auto`) and per utility for a value chosen as the page renders or per breakpoint: `gap(Responsive::new(Space::_2).md(Space::_4))` prints `gap-2 md:gap-4` |
| `App::components(&[TOKENS, OfferList, ...])` | one `components.css` in the listed order, linked from `<head>` and precached by the PWA; a component rendered but not listed is an error under `corten dev` |
| `var(--color-accent)` in a component's stylesheet | a startup error unless a stylesheet on the same pages declares it (the token file, a listed component's, a `.css` file in the static directory, or for an area's component one linked from its area), naming the file and the property; `var(--x, fallback)` is not checked |
| `App::area(\|area\| admin::area(area).components(&[SalesChart, ...]))` | those stylesheets in a file of their own, `sections/admin.css`, linked only from pages at and under `/admin`; a component listed in two places, or tokens in an area, is a startup error, and a page outside the area that renders one is an error under `corten dev` |

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

A misspelled token (`var(--color-primary-contrast)` where the file has
`--color-primary-foreground`) would make the declaration invalid and the property inherit, which
shows only as a wrong color. So when the app starts (`App::router`, which every test of the app
builds), each `var(--x)` without a fallback in a listed stylesheet must name a property declared
by a stylesheet on the same pages, or it fails with the file and the property. It is a startup
check, not a compile error, because a component's stylesheet compiles without seeing the theme,
which may be another crate's. A property a script sets (`el.style.setProperty`) takes a fallback,
`var(--progress, 0)`.

`static/style.css` holds what belongs to no component: element defaults, fonts and the layout.
Link it before `<Corten />` in the layout, which links the components' stylesheet, so component
rules win ties. Inline `style=` stays blocked by the
CSP; one-off values come from the token utilities instead. Pages that must draw a size from data
(a meter's fill, marks on a timeline) are an area that allows style attributes,
`.policy(|p| p.allow_style_attributes())` ([app guide](app.md#a-policy-for-part-of-the-app)), so
the rest of the app keeps the protection.

## Printing

A page prints with its components' own print rules. An `@media print` block is scoped like any
other, each rule starting at one of the file's classes. A named `@page` gives the paper size to
the elements that ask for it with `page`:

```css
/* src/views/packing_slip.css: PackingSlip, so its classes are packing-slip__* */
@page packing-slip-thermal { size: 4in 6in; margin: 0.15in; }
@page packing-slip-letter { size: letter; margin: 0.5in; }
.slip { break-after: page; }
.thermal .slip { page: packing-slip-thermal; }
.letter .slip { page: packing-slip-letter; }
@media print { .actions { display: none; } }
```

The format the user chose puts one class on the list, so one job prints at one paper size. A
page's name is the whole app's, so a component's starts with the component's own (`@page
packing-slip-thermal`), and an `@page` with no name, which sets every page the app prints, does
not compile in a component's stylesheet: it goes in `static/style.css`, with resets for the
elements no component owns (`main`'s margins). `page` applies to boxes in normal block flow, so
an element that picks a page is not a flex or grid item. A component's stylesheet cannot reach the
layout's header or navigation, so each layout component hides itself with its own `@media print`
rule. corten-ui's `PrintButton` opens the print dialog (wrap it in one of your component's
elements to hide it in print); without a script, the browser's Print command prints the same page.

## Utility-first

A page can be styled mostly in its markup, as Tailwind does, from the token file's utilities.
`examples/watchlist` is: its `tokens.json` defines utilities for layout, spacing, color, borders
and type, and `src/theme.rs` names the few combinations it repeats as functions:

```rust,ignore
pub fn card() -> impl ClassList {
    (bg_surface(), border_line(), rounded_lg(), shadow_sm(), p_5())
}

/// A row of items, `space` apart: a value chosen by the caller goes to the utility's function.
pub fn row(space: Space) -> impl ClassList {
    (display_flex(), items_center(), gap(space))
}

<div class={(display_grid(), columns(Responsive::new(Columns::_1).md(Columns::Main)), gap_5())}>
    <section class={card()}>..</section>
</div>
```

Each value of a utility is a function named after its class, so `mx_auto()` is `mx-auto`, as in
Tailwind. The utility's own function, `gap(..)`, is for a value the code chooses (`gap(space)`)
and for one per breakpoint (`Responsive::new(..).md(..)`).

Unlike Tailwind's class strings, a utility the file lacks, a value outside its scale or a
breakpoint it does not define does not compile, and there is no build step or content scan: the
token file generates every class it allows, so the stylesheet holds them all whether a page uses
them or not. Measured on the watchlist: its `components.css` (the tokens' utilities and the price
table's stylesheet) is 16,282 bytes, 2,042 brotli at level 11, against 1,063 brotli for the
card market's tokens and five components; a scale of eight spaces in four breakpoints is 32 classes
per utility, so a utility belongs in the file only when pages use it.

Utilities also cost at render time, where a component's classes do not: `class={Card}` is
static text written when the crate compiles, while `gap(Space::_3)` is a value written as the
page renders. Measured with callgrind (release build, in-process `oneshot`, the difference between
1,200 and 200 requests over 1,000, median of three runs), moving the watchlist's home page to
utilities took it from 45,952 instructions a request to 51,201, with a larger page; a profile put
3,086 of the difference in writing its roughly 60 utility classes. Use
utilities for layout that varies from page to page, and a component's stylesheet for what repeats.
A value's own function (`gap_3()`) is its class as static text, with no match: moving the
watchlist's fixed values to them took its home page from 62,827 instructions a request to 62,162.
A helper that takes a value (`stack(space)`) keeps the utility's function, which costs a match
and keeps the scale's type. States a utility cannot
say (hover, focus, the current link) and element defaults stay in `static/style.css`, and a
component with rules of its own (`LivePrices`) keeps its stylesheet. A utility function's
classes are static text chosen by a match, so writing them costs what writing a class does.
`use theme::*` brings in every value's function; an item of the module's own with the same name
hides it.

## Areas

Every page links `components.css`, so a large part of the app that most visitors never open (an
admin console, seller tools) adds its CSS to every first load. An area
([app guide](app.md#areas)) gives that part its routes under one prefix and
a stylesheet of its own:

```rust,ignore
App::new(routes)
    .components(&[TOKENS, Button, OfferList])                              // every page
    .area(|area| admin::area(area).components(&[SalesChart, AuditTable])) // pages at /admin and under it
```

- The server picks a page's stylesheets by its path, from links built at startup: no per-render
  tracking. A page under nested areas (`/admin`, `/admin/reports`) links each, outermost first.
- A client navigation into the area adds the stylesheet to `<head>` and waits for it before
  swapping the page in, so the page never shows unstyled; if it fails to load, the runtime loads
  the page in full. The stylesheet then stays: its rules match only its components' classes.
- A component that pages in several areas render is listed in each of those areas: its rules
  go in a file the areas share (`sections/search+categories.css` for one screen at `/search`
  and `/categories/{slug}`), linked from their pages only, with any other component the same
  areas list. One most pages render belongs in `App::components` instead, and so do tokens,
  which style every page; a component is in one or the other, not both. A patch is styled by the page it lands on and is checked
  against its own URL, so a form whose action sits outside the page's area should answer with
  components from `App::components`.

Splitting has a price: two files compress worse than one, and entering the area costs a
request. Measured on the item page (release build, brotli at its file level): one
`components.css` is 3,223 bytes, 1,047 brotli. With the item page's four components in an
`/items` area, every page gets 593 bytes brotli and item pages 548 more, so the home page
saves 454 bytes and an item page downloads 94 more, in two requests. Leave everything in
`App::components` until an area's share is large enough to measure on the pages outside it.

The same holds for one page: the home page, or a page whose path is the prefix of other areas
(`/account` above `/account/cart`). An area at its path would link its stylesheet from every page
under it too, so its components go in `App::components`. Measured in the Chase Sets pilot
(release build, brotli): the account page's 3,737 bytes of CSS add 373 bytes to `components.css`,
downloaded once and then cached, where an `/account` area's file would be 837 bytes and one more
render-blocking request on every account screen. A page whose own CSS is tens of KB compressed
would change the numbers; that case gets its own measurement.

Without areas a request costs what it did before (callgrind, the item page in process: the
same instruction counts within run-to-run noise). Writing an area's links costs about 120
instructions a page; the item page's navigation as a whole measured 88,341-88,589 instructions
with an area against 87,249-87,331 without; most of the rest of the difference is in serde_json,
which the area code does not call, and the allocator. Area stylesheets add 126 bytes gzip to the runtime.
