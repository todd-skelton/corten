# The UI kit

[Back to the guides](../../README.md#documentation)

`corten-ui` is headless: it gives the browser's missing widgets their behavior and leaves their
look to the app. The kit is that look, ready-made: theme tokens and styled components that
`corten ui` copies into the app as source, so the app owns them, edits them like its own code, and
the framework keeps no styled API to version. The components are ordinary `#[component(css)]`s
and the tokens an ordinary `tokens!` file, so nothing here is a second way to write a view.

```sh
corten ui init            # the theme tokens and src/ui.rs
corten ui add button card # src/ui/button.rs, button.css, card.rs, card.css
corten ui list            # every component the kit has
corten ui diff            # how the copies differ from this command's kit
```

Each command works out every change first and writes them all or none: each file is written
beside its place first (`.<name>.corten-ui`, refused if the app has a file by that name), and
only when all are written does each take its place; when one cannot, those placed before it are
put back, so running it again starts over (a file that cannot be put back is named). It adds
`corten-ui` from where `corten` comes only when `corten`'s line in `Cargo.toml` says so on
that line; otherwise it says to add it by hand.

| Command | Writes |
|---|---|
| `corten ui init` | `tokens/ui.tokens.json` (light), `tokens/ui-dark.tokens.json` and `tokens/ui.resolver.json` (the theme modifier, following the system when the page chose no theme), `src/ui.rs` (the tokens and the `COMPONENTS` list), the page's colors at the end of `static/style.css` (unless it sets `color-scheme` already, when it says what to add); `pub mod ui;` among the crate root's `mod` lines (`src/lib.rs`, else `src/main.rs`) and `.components(ui::COMPONENTS)` on the crate's one `App::new(..)` line, in whichever file builds the `App` (`crate::ui::COMPONENTS` from another module). An app with colors of its own (a `tokens!` theme with a color token) keeps them: `init` writes only `src/ui.rs`, without the kit's tokens, and names the tokens the components read that the theme lacks ([below](#an-apps-own-theme)); `corten ui init --kit-theme` writes the kit's theme beside it instead. A binary that builds its `App` beside a `src/lib.rs` is told to add the line by hand. It refuses to run twice, and says what to do by hand when it cannot place a line |
| `corten ui add <name>..`, `--all` | each component's files in `src/ui/`, `pub mod <name>;` and the component in `src/ui.rs`'s `COMPONENTS`, laid out as rustfmt does; for a component that styles a corten-ui widget, `corten-ui` in `Cargo.toml` from where `corten` comes (next to a local `corten`, the same Git repository and revision, else this command's release) and `.components(corten_ui::COMPONENTS)` on the `App`, unless a file of the crate lists it already. A file that exists is refused, before anything is written, unless `--force`; so is a component the app has by the same name with a stylesheet of its own (a `Field` in `src/layout.rs`), naming the file, since the two would style one class prefix and the program would not link |
| `corten ui diff [name]..` | nothing: for each copied file (of the components named, or all of `src/ui/`), whether the app edited it, whether the kit changed it since the copy, and a unified diff from the copy to the kit this command carries. A file the kit changed and the app did not is the kit's to replace (`corten ui add --force`); one both changed is merged by hand |

Each file starts with the kit version it came from and a hash of the file as written
(`// corten ui 0.1.0: button 71f206bc167066a1`), which is how `corten ui diff` tells the app's edits
from the kit's changes; a file without the line is compared whole. The files are embedded in the `corten` binary, so adding one needs no network, and a component is the
one the command's version ships.

## Theme

The token names are shadcn/ui's, so a theme made for it maps one variable to one token:
`--background`, `--foreground`, `--card`, `--popover`, `--primary`, `--secondary`, `--muted`,
`--accent`, `--destructive` (each with a `-foreground` for the text on it), `--border`,
`--input`, `--ring` and `--radius`, and three tones for status that shadcn/ui leaves to the app,
`--info`, `--success` and `--warning` (each with a `-foreground`), which `Badge` and `Alert`
use. They are tokens at the top of the file, so each is a custom property with no Rust enum, and
none has a scale's name (`--color-accent`, `--radius-sm`), so the kit's theme and an app's can
be on one page without one replacing the other's values; only a token of the app's whose property has
one of these names would be the same property: `card` at the top of a file, a `card` group's
`$root`, or `muted.foreground` (`--muted-foreground`). Status tones have no `-soft` token: an
alert's tinted background is the tone mixed into `--card` in its stylesheet (`color-mix`), as
`destructive`'s is, so a theme sets one color per tone. To bring a theme over, write each of its
colors into `ui.tokens.json` (`:root`'s) and `ui-dark.tokens.json` (`.dark`'s) as the [styles
guide](styles.md#design-tokens) writes colors (sRGB with its hex, or `oklch`); the startup check
fails, naming the file and property, if a component reads one the theme lacks. The kit's own
look is warm grays with a weathering-steel orange as `--primary` and `--ring`. Its text meets
WCAG AA contrast in both themes, which a test of `corten ui` checks: each `-foreground` on its
color, each tone on a card (an alert's text) and the muted text on the page and on a card 4.5:1,
and `--input` (a field's border) 3:1 on the page.

Sizes are tokens too, so a theme changes them in one place and the copied files stay the kit's:
`--control-height` (a button, field, select, menu button or page link of the default size; 2.25rem),
`--control-height-sm` and `--control-height-lg` (2rem and 2.5rem, a small or large button and a
tab), `--control-padding` (a field's inline padding; .75rem), `--control-font-size` (.875rem),
`--card-padding` (1.5rem) and `--heading-font-size` (a card's heading; 1rem). A 44 px touch
target is `"control-height": { "$type": "dimension", "$value": { "value": 2.75, "unit": "rem" } }`;
a compact density is a modifier context of the app's that sets them lower. A size is the same in
both themes, so `ui-dark.tokens.json` has none.

A focused field's ring is a box shadow, which forced colors (Windows High Contrast) does not
paint, so each text control and menu item also has a transparent outline that forced colors
paints in the system's color: a field shows its focus in either.

### An app's own theme

An app whose `tokens!` theme has colors when it runs `corten ui init` keeps it, and the components
read their tokens from it: `init` reads the theme's files and writes `src/ui.rs` with no tokens of
the kit, and names the tokens the components read that the theme lacks. (A theme with no colors, a
spacing scale alone, gets the kit's theme beside it, as an app with none does. With several
`tokens!` themes, the first with colors that `init` finds is the one it reads.) Declare each at the top of the app's token file, most as a reference to a token
the app has, so it follows the app's other themes (dark, a density) with nothing more:

```json
{
  "color": { "$type": "color", "surface": { "$value": { "colorSpace": "srgb", "components": [1, 1, 1], "hex": "#ffffff" } } },
  "card": { "$type": "color", "$value": "{color.surface}" }
}
```

The app does not start while a component reads a token no stylesheet declares, and the error names
the stylesheet and the token, so a missing one is found at the first run.

When a token the components read has the name of one of the theme's groups (a `radius` group for
`--radius-sm`), `init` says so: the group's `$root` is the group's own property, so
`"radius": { "$root": { "$value": "{radius.md}" }, "sm": .. }` declares `--radius` and keeps
`--radius-sm`. A theme `init` cannot read (a file its resolver names
is missing) is reported, and nothing is written; `--kit-theme` takes the kit's theme instead,
beside the app's.

A utility over the kit's tokens, a text color for each tone (`fg_success()`), lists them as its
values, which gives it a Rust enum as a group does:

```json
"$extensions": {
  "run.corten": {
    "utilities": {
      "fg": { "property": "color", "values": { "muted": "var(--muted-foreground)", "success": "var(--success)", "warning": "var(--warning)" } }
    }
  }
}
```

on the token file's root, as the [styles guide](styles.md#design-tokens) places utilities.

## Components

A component that only styles an element is a look ([styles guide](styles.md#looks)): its classes go
on the element the app writes, `<button class={Button { .. }}>`, so `html!` checks the element's
attributes as it checks any (`type`, `disabled`, `href={url::Home}`, `name={f.email}`,
`on:click`, `commandfor`), the element can be the one the page needs (a card as a `<section>`,
an `<li>` or a patch target, a badge with a `title`), and the page has no element of the kit's
around it, so a script's `parentElement` and a `:first-child` rule see the page as written. A
component that draws structure (a field's label and hint, a checkbox's label) or semantics (an
alert's role) renders its elements. A corten-ui widget whose button is its own takes the look as
its `class` (`<CopyButton .. class={Button { .. }} />`).

```rust,ignore
use crate::ui::{button::*, card::*, field::*, input::*};

<section class={Card {}}>
    <header class={CardHeader {}}><h2>"Sign in"</h2><p>"Use the email you registered with."</p></header>
    <div class={CardContent {}}>
        <Field id={EmailId} label="Email" hint="" error={e.message(f.email)}>
            <input class={Input {}} id={EmailId} type="email" name={f.email} required {..e.aria(f.email)}>
        </Field>
    </div>
    <footer class={CardFooter {}}>
        <button class={Button { variant: ButtonVariant::Primary, size: ButtonSize::Md }}>"Sign in"</button>
        <a class={Button { variant: ButtonVariant::Link, size: ButtonSize::Md }} href={url::Reset}>"Forgot it?"</a>
    </footer>
</section>
```

Each component's keyframes start with its own name (`dialog-in`), as an `@page` name does, since
they are the whole page's.

| Component | Renders | Variants |
|---|---|---|
| `Button` | a look: the classes of the app's `<button>` or `<a>` (`class={Button { variant, size }}`) | `ButtonVariant`: `Primary`, `Secondary`, `Outline`, `Ghost`, `Destructive`, `Link`; `ButtonSize`: `Sm`, `Md`, `Lg`, `Icon` |
| `Badge` | a look: the classes of the app's label element (`<span class={Badge { tone, emphasis }} title={..}>`); each tone and emphasis meets WCAG AA in both themes, which the kit's test checks | `BadgeTone`: `Neutral`, `Primary`, `Info`, `Success`, `Warning`, `Destructive`; `BadgeEmphasis`: `Solid`, `Soft`, `Outline` |
| `Card`, `CardHeader`, `CardContent`, `CardFooter` | looks: a surface and its parts, on the app's elements (`<section class={Card {}}>`, `<header class={CardHeader {}}>`); the heading and description in the header are the app's (`<h2>` or `<h3>`, as the page's outline needs, then `<p>`) | |
| `Alert` | a box whose role follows its variant, so a screen reader interrupts only for what needs the user now: `role="alert"` for `Warning` and `Destructive` (read at once when a patch brings it in), `role="status"` for `Success` (read politely; one brought in with its text is not read by every screen reader, so the result of an action is a toast, corten-ui's `Toaster`), none for `Neutral` and `Info` (read in the page's order) | `AlertVariant`: `Neutral`, `Info`, `Success`, `Warning`, `Destructive` |
| `Field` | a `<label for={id}>` naming the control by its `#[derive(Id)]` type (a corten-ui widget takes `QtyId::NAME`), the control (a kit control or a corten-ui widget, its parts in one row), the hint (its id `FieldHintId(&id)`, for the control's `aria-describedby` or `e.aria(..).hint(..)`), then the server's message for the field (`error={e.message(f.email)}`), drawn destructive; `""` draws none | |
| `Input`, `Textarea`, `Select`, `Slider` | looks: the classes of the app's `<input>`, `<textarea>`, `<select>` or `<input type="range">` (`class={Input {}}`), with the destructive border once the user has edited an invalid value (`:user-invalid`) or the server marked it (`aria-invalid`, which `e.aria(field)` writes); `Select` styles the open list where the browser has `appearance: base-select` | |
| `Checkbox`, `Switch` | a `<label>` with the `<input type="checkbox">` inside and its text after; a switch is `role="switch"` on the checkbox, so it posts as one | |
| `RadioGroup`, `Radio` | a `<fieldset>` with its `<legend>`, and each `<input type="radio">` with its text | |
| `Separator` | an `<hr>` | `Orientation`: `Horizontal`, `Vertical` |
| `Skeleton` | a placeholder's shape, hidden from screen readers, still under `prefers-reduced-motion` | `SkeletonShape`: `Line`, `Block`, `Circle` |
| `Table` | the look of the `<table>` inside it, scrolling sideways when it is wider than the page | |
| `Breadcrumb` | a `<nav>` of typed links to the pages above (`Crumb { label, link: &url::Sets }`), then the page, `aria-current="page"` | |
| `Pagination` | a `<nav>` of links to a list's pages, each from `link(n)`, a typed route; the first, the last and two each side of the current page, Previous and Next disabled at the ends | |
| `Avatar` | a person's picture over their initials, which show while it loads and when there is none; `alt` names the person on the picture, or on the avatar (`role="img"`) when there is none | |
| `Dialog`, `DialogFooter` | the look of a `<dialog>` (`<dialog class={Dialog {}}>`), its `::backdrop` and its actions (`<div class={DialogFooter {}}>`); a button opens it with `command="show-modal" commandfor={..}` and closes it with `command="close"`, with no script | |
| `Sheet` | the same, sliding in from an edge of the window (`<dialog class={Sheet { side: Side::Right }}>`) | `Side`: `Top`, `Right`, `Bottom`, `Left` |
| `Popover` | the look of a `popover` element (`<div class={Popover {}} popover>`), opened by `popovertarget`, under its button where the browser has anchor positioning | |
| `Accordion` | the look of each `<details name>` (`<details class={Accordion {}} name="faq">`), which open one at a time, and find-in-page opens the one with the match | |
| `Tabs`, `DropdownMenu`, `Combobox`, `Toasts` | the look of corten-ui's `TabGroup`, `MenuButton`, `ComboBox` and `Toaster` inside them, by role and state (`[role="tab"][aria-selected="true"]`, `[role="menu"]:popover-open`, `[role="option"][data-highlighted]`); `Toasts` places the toaster at the bottom right. `corten ui add` makes the app depend on `corten-ui` and list `corten_ui::COMPONENTS` | |

Variants are Rust enums matched to the stylesheet's classes, so a variant without a rule does
not compile, and every prop is written where the component or look is used, as for any component
([views](views.md)). The classes are static text chosen by a `match`: a page pays for joining two
or three of them, not for any lookup. No component has a script: each works without
JavaScript and under the CSP, with no `style=`, and the labels they show (Previous, Next, a
legend) are the app's, in its language.

Every component is in use in the [ui-gallery](../../examples/ui-gallery) example, built from the
kit's own files, and its browser test (`examples/ui-gallery/smoke.mjs`) runs them in Chromium and
WebKit, with and without JavaScript.

## Size

The kit's CSS goes into `components.css` with the app's own, one cached file. Measured on a new
app with every component added (`corten ui add --all`) and nothing of its own but the starter's
counter: 26,566 bytes, 4,104 with brotli at level 11 and 4,673 with gzip at level 9, the theme's
tokens included. An app pays for the components it adds, and `corten ui add` adds one at a time.
