# Styles

[Back to the guides](../../README.md#documentation)

Each component's styles are a CSS file next to it, scoped to it at compile time; design tokens
are W3C design-token files; both reach the browser as one cached stylesheet. There is no CSS
build step.

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
| `#[component(css = OfferList)]`, `css = super::offers::OfferList` | a helper component shares the owner's stylesheet from any module of the crate that can see the owner, named by its path (not through a `use`) (a screen's panels in files of their own), and its uses count; classes are in scope only in a component, so a plain `fn` that picks a class is a helper component or a `match` in the markup |
| `tokens!("tokens/tokens.resolver.json")` ([design tokens](#design-tokens)) | custom properties (`--space-4`), each theme's changes (`[data-theme="dark"]`, `[data-density="compact"]`), enums (`Space::_4.var()`) and typed utilities, a function per value (`gap_2()` prints `gap-2`, `mx_auto()` `mx-auto`) and per utility for a value chosen as the page renders or per breakpoint: `gap(Responsive::new(Space::_2).md(Space::_4))` prints `gap-2 md:gap-4` |
| `App::components(&[TOKENS, OfferList, ...])` | one `components.css` in the listed order, linked from `<head>` and precached by the PWA; a component rendered but not listed is an error under `corten dev` |
| `var(--color-accent)` in a component's stylesheet | a startup error unless a stylesheet on the same pages declares it (the token file, a listed component's, a `.css` file in the static directory, or for an area's component one linked from its area), naming the file and the property; `var(--x, fallback)` is not checked |
| `App::area(\|area\| admin::area(area).components(&[SalesChart, ...]))` | those stylesheets in a file of their own, `sections/admin.css`, linked only from pages at and under `/admin`; a component listed in two places, or tokens in an area, is a startup error, and a page outside the area that renders one is an error under `corten dev` |

Variants are Rust: `match tone { Tone::Danger => Danger, Tone::Neutral => Neutral }` picks the
class, so adding a variant without a rule does not compile. A component library is a crate that
exports `pub const COMPONENTS: &[Component]`; the app passes it to `App::components`.

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
rules win ties. The components' stylesheet starts with `[hidden] { display: none !important }`:
a part hidden with the attribute (`hidden={..}` in markup, `el.hidden = true` from a script)
stays hidden even when one of its classes gives it `display: grid`, which would otherwise win
over the browser's own rule. `hidden="until-found"` is left to the browser, so find-in-page can
still reveal it. When only areas have stylesheets, each area's file starts with the rule. An
app with no component stylesheet has neither file, so if its `static/style.css` gives a hidden
element a `display` (styling corten-ui's parts there, say), it adds the rule itself. Inline
`style="..."` stays blocked by the CSP; one-off values come from the token utilities, and a size
drawn from data from a custom property ([below](#values-from-data)).

## Values from data

A size or position the page computes (a meter's fill, a bar's length, a mark on a timeline) is a
custom property the component's stylesheet registers with `@property` and reads with `var()`,
set in its markup with `style={..}`. `examples/seller-desk`'s statement draws each set's share of
the stock's value this way:

```css
/* statement_table.css */
@property --share { syntax: "<percentage>"; inherits: false; initial-value: 0%; }
.bar { width: 6rem; background: linear-gradient(to right, var(--accent) var(--share), var(--tint) var(--share)); }
```

```rust,ignore
<span class={Bar} style={SHARE.pct(share)}></span>
```

| You write | Corten checks or generates |
|---|---|
| `@property --share { syntax: "<percentage>"; .. }` | the constant `SHARE: Var<Percentage>` in the component's markup, and `--statement-table__share` everywhere in the file: the property is the component's, as its classes are |
| `SHARE.pct(12.5)`, `GAP.px(4.0)`, `TURN.deg(90.0)`, `SPAN.set(3)` | a method per unit the syntax allows (every CSS unit but `in`, a Rust keyword: an inch is `px(96.0)`) (`<number>`, `<integer>`, `<percentage>`, `<length>`, `<length-percentage>`, `<angle>`, `<time>`); another unit, CSS text (`style={format!(..)}`) or another component's property does not compile, and a number that is not finite sets nothing, so the property keeps its `initial-value` |
| `style={(SHARE.pct(s), nudge.map(|n| GAP.px(n)))}` | one attribute, `data-vars="--statement-table__share:12.5%"`; `None` sets nothing, and with nothing to set there is no attribute |
| an `@property` without `syntax`, `inherits` or `initial-value`, or with a number syntax and an `initial-value` that is not one, or depends on the font or viewport (`1em`) | a compile error: the browser would ignore the rule; another syntax's `initial-value` (`<color>`) is left to the browser |

A full page gets one `<style>` at `<Corten />`, after its stylesheets, with a rule for each
distinct value (`[data-vars="--statement-table__share:12.5%"]{..}`), marked `!important` so it
wins over the component's own rules as the runtime's inline values do, and that response's CSP
allows it by its hash in `style-src`. So the page paints with its sizes and no script, no style
attribute is allowed anywhere, and a cached page is still the same bytes (no nonce). Markup a patch,
a client navigation or a `<corten-live>` fragment brings is under the policy the document loaded
with, which allows neither, so the runtime sets those values through the CSSOM
(`el.style.setProperty`), which the policy does not govern, before the browser paints them. A row
`rows()` copies out of a `<template>` gets the server's values for the template's row the same
way, not its own: a script that fills rows draws their sizes itself. A property registered with another syntax
(`<color>`, to animate it) is scoped the same way and has no constant.

The block costs a page that uses it the hash of its text: measured with callgrind (release
build, in process), four meters with three distinct values took a page from 21,911 instructions a
request to 40,115, about 8,400 of it the SHA-256. A page that sets none runs about 130 more than
before. Each distinct value is written twice in the block and once in its attribute, which
compresses well when values repeat; a chart of hundreds of distinct values belongs in SVG
attributes, which need no CSS at all.

An area that allows style attributes, `.policy(|p| p.allow_style_attributes())`
([app guide](app.md#a-policy-for-part-of-the-app)), is for markup the app does not write as
`html!`, such as rich text from a CMS.

## Design tokens

Token files are the W3C Design Tokens format, 2025.10: the files Figma variables, Tokens
Studio, Style Dictionary and Terrazzo read and write, so a design tool's export needs no
converter and the same files can feed iOS and Android builds. A token file holds the values; a
resolver document (`.resolver.json`) puts token files together and gives the themes.
`tokens!("tokens/tokens.resolver.json")` reads a resolver (a file with `resolutionOrder`) and
the files it names, each a path relative to the file that names it (a URL, a scheme, a drive or
an absolute path does not compile; `#/` is a whole file, and `example.json#sets/MySet`, as the
resolver module also writes it, is `#/sets/MySet`; a JSON pointer, like a `{group.token}`
reference, names a token in the tokens put together, as the resolver module resolves aliases
only after the sources are merged, so a later source's value wins; in a source that is part of a
file, a pointer into the part is moved to where the part goes, and one outside it is the value it
names); `tokens!("tokens/base.tokens.json")` reads one token file, for an app with no themes.

```json
{
  "color": {
    "$type": "color",
    "ink": { "$value": { "colorSpace": "srgb", "components": [0.1137, 0.1059, 0.149], "hex": "#1d1b26" } },
    "link": { "$value": "{color.ink}" }
  },
  "space": {
    "$type": "dimension",
    "4": { "$value": { "value": 16, "unit": "px" } }
  },
  "$extensions": {
    "run.corten": {
      "breakpoints": { "md": { "value": 48, "unit": "rem" } },
      "utilities": { "gap": { "property": "gap", "group": "space" } }
    }
  }
}
```

Each top-level group's tokens are a scale: `--color-ink` and `Color::Ink`, `--space-4` and `Space::_4` (the group's own `$root`, below, is not in it). A
nested group joins its names with `-` (`--color-brand-primary`). Names are written as CSS
writes them, so a design tool's `Brand Primary` or `brandPrimary` is `--brand-primary` (and
`BrandPrimary` in Rust); two names that come out the same, `font-size` and `FONT-SIZE`, do not
compile, a letter outside ASCII is an error, and a top-level group (a Rust enum), breakpoint or
utility (a CSS class) starts with a letter. A token at the top of the file, or a top-level
group's `$root` (`--brand`), is a custom property with no enum; a `$root` on the file itself has
no name to give it, so it is an error. Values are written in CSS: an sRGB color whose
`hex` agrees with its components is the hex, other color spaces are CSS Color 4 (`oklch(..)`,
`color(display-p3 ..)`), and dimensions, durations, shadows, borders, transitions, cubic Béziers,
font families and weights have one CSS value each. A reference stays one: `"{color.ink}"` is
`var(--color-ink)`, so a theme that changes `ink` changes `link` with it. A JSON pointer into a value (`{"$ref":
"#/space/4/$value/value"}`) is the number or text it points at, written where it is used,
following a token that is itself a reference to the one with a value; a pointer is a URI fragment,
so `#/Brand%20Primary` names `Brand Primary`, and a font list names a token by pointer
(`[{"$ref": "#/font/brand/$value"}, "serif"]`), as the schema has it. A circle of references
does not compile, and a chain of more than 256 pointers, or of groups extending one another, is an error (a chain of
aliases, `{a}` naming `{b}`, can be any length), as are a number too large for an `f64` (CSS writes finite numbers), objects and arrays nested
more than 128 deep, and following more than 100,000
JSON pointers, `$extends`, sets and file references in all (a file naming one set twice at each of many levels). A
gradient (it has no direction) and a typography token (it is several properties) have no one CSS
value, so they do not compile; give their parts tokens of their own. A stroke style of dashes and
caps is `dashed`, the closest CSS border style, as the Format module suggests.

The themes are the resolver's modifiers:

```json
{
  "version": "2025.10",
  "sets": { "base": { "sources": [{ "$ref": "base.tokens.json" }] } },
  "modifiers": {
    "theme": {
      "contexts": { "light": [], "dark": [{ "$ref": "dark.tokens.json" }] },
      "default": "light",
      "$extensions": { "run.corten": { "media": { "dark": "(prefers-color-scheme: dark)" } } }
    }
  },
  "resolutionOrder": [{ "$ref": "#/sets/base" }, { "$ref": "#/modifiers/theme" }]
}
```

A modifier's `default` is the values on `:root`; the format lets a modifier leave it out and
asks for an input instead, but a page that chooses no context still needs values, so here it is
required. Each other context's changes apply under
`[data-theme="dark"]` (the modifier's name and the context's, written as CSS names), on the
document or any element inside it, so `dark.tokens.json` holds only the values that change.  The
default context has a rule too, `[data-theme="light"]`, with the default values of what the
others change, so a light panel inside a dark page, or on a system that is dark, is light. A
token that reads one a context changes is declared again under each context, so it follows on an
element too. Breakpoints are wider than 0 and go smallest first, in one unit, or the file does not compile; a
utility's `group` is a top-level group's name as the file writes it (`"Brand Space"`, whose
classes and properties are `brand-space`). A context that defines a
token the defaults lack, or gives one another type, does not compile: the page that chose no
context would have no value for it. Two modifiers' contexts on one element apply in
resolutionOrder, the later winning, and each combination (a chosen default included, whose rule
declares again what its modifier's other contexts change) is checked against what the resolver
gives: two modifiers that change one token, or one that sets a token by value that reads a token
another changes, do not compile. Give each modifier tokens of its own and reference them. The
check resolves each combination, each modifier's attribute absent or one of its contexts, up to
256 of them (two modifiers of up to 15 contexts each, or four of three), and more is an error. `media` lets a context follow the system: the page is dark
when it says `data-theme="dark"`, or when the system is dark and the page chose no theme
(`:where(:root):not([data-theme])` under the condition, with the specificity of one attribute,
as each context's rule has, so a later modifier still wins), with the values written once. `color-scheme`
(which themes form controls and scrollbars) is a property, not a token, so it goes in
`static/style.css` next to the same selectors.

Corten's own settings are under `$extensions`, key `"run.corten"` (corten.run, reversed, as the
format asks of a tool's key): breakpoints and utilities on a token file's root group, `media` on
a modifier. Anywhere else it is an error, and other tools' keys are left alone.

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
`examples/watchlist` is: its `tokens/base.tokens.json` defines utilities for layout, spacing, color, borders
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
