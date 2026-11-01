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

| Command | Writes |
|---|---|
| `corten ui init` | `tokens/ui.tokens.json` (light), `tokens/ui-dark.tokens.json` and `tokens/ui.resolver.json` (the theme modifier, following the system when the page chose no theme), `src/ui.rs` (the tokens and the `COMPONENTS` list), the page's colors at the end of `static/style.css`; in `src/lib.rs`, `mod ui;` and `.components(ui::COMPONENTS)` on the `App::new(..)` line. It refuses to run twice, and says what to do by hand when `src/lib.rs` or `static/style.css` is not where `corten new` puts them |
| `corten ui add <name>..`, `--all` | each component's files in `src/ui/`, `pub mod <name>;` and the component in `src/ui.rs`'s `COMPONENTS`, laid out as rustfmt does. A file that exists is refused, before anything is written, unless `--force` |
| `corten ui diff [name]..` | nothing: for each copied file (of the components named, or all of `src/ui/`), whether the app edited it, whether the kit changed it since the copy, and a unified diff from the copy to the kit this command carries. A file the kit changed and the app did not is the kit's to replace (`corten ui add --force`); one both changed is merged by hand |

Each file starts with the kit version it came from and a hash of the file as written
(`// corten ui 0.1.0: button 71f206bc167066a1`), which is how `corten ui diff` tells the app's edits
from the kit's changes; a file without the line is compared whole. The files are embedded in the `corten` binary, so adding one needs no network, and a component is the
one the command's version ships.

## Theme

The token names are shadcn/ui's, so a theme made for it maps one variable to one token:
`--background`, `--foreground`, `--card`, `--popover`, `--primary`, `--secondary`, `--muted`,
`--accent`, `--destructive` (each with a `-foreground` for the text on it), `--border`,
`--input`, `--ring` and `--radius`. They are tokens at the top of the file, so each is a custom
property with no Rust enum. To bring a theme over, write each of its colors into
`ui.tokens.json` (`:root`'s) and `ui-dark.tokens.json` (`.dark`'s) as the [styles
guide](styles.md#design-tokens) writes colors (sRGB with its hex, or `oklch`); the startup check
fails, naming the file and property, if a component reads one the theme lacks. The kit's own
look is warm grays with a weathering-steel orange as `--primary` and `--ring`; each pair of
text and background meets WCAG AA contrast (4.5:1) in both themes, and `--input` (a field's
border) 3:1.

## Components

A component styles the element the app writes inside it when the element takes attributes the
app chooses, so `html!` checks them as it checks any element (`type`, `disabled`,
`href={url::Home}`, `name={f.email}`, `on:click`, `commandfor`), and the wrapper is
`display: contents`, adding no box. A component that is only a surface renders its own element.

```rust,ignore
use crate::ui::{button::*, card::*, field::*, input::*};

<Card>
    <CardHeader><h2>"Sign in"</h2><p>"Use the email you registered with."</p></CardHeader>
    <CardContent>
        <Field id="email" label="Email" note={e.message(f.email)}>
            <Input><input id="email" type="email" name={f.email} required {..e.aria(f.email)}></Input>
        </Field>
    </CardContent>
    <CardFooter>
        <Button variant={ButtonVariant::Primary} size={ButtonSize::Md}><button>"Sign in"</button></Button>
        <Button variant={ButtonVariant::Link} size={ButtonSize::Md}><a href={url::Reset}>"Forgot it?"</a></Button>
    </CardFooter>
</Card>
```

Each component's keyframes start with its own name (`dialog-in`), as an `@page` name does, since
they are the whole page's.

| Component | Renders | Variants |
|---|---|---|
| `Button` | the look of the `<button>` or `<a>` inside it | `ButtonVariant`: `Primary`, `Secondary`, `Outline`, `Ghost`, `Destructive`, `Link`; `ButtonSize`: `Sm`, `Md`, `Lg`, `Icon` |
| `Badge` | a `<span>` label | `BadgeVariant`: `Primary`, `Secondary`, `Outline`, `Destructive` |
| `Card`, `CardHeader`, `CardContent`, `CardFooter` | a surface and its parts; the heading and description in the header are the app's (`<h2>` or `<h3>`, as the page's outline needs, then `<p>`) | |
| `Alert` | a `role="alert"` box, so a screen reader reads it when a patch brings it in | `AlertVariant`: `Neutral`, `Destructive` |
| `Field` | a `<label for={id}>`, the control (a kit control or a corten-ui widget, its parts in one row), then a note: a hint in text, or the server's message for the field (`note={e.message(f.email)}`), drawn destructive | |
| `Input`, `Textarea`, `Select`, `Slider` | the look of the `<input>`, `<textarea>`, `<select>` or `<input type="range">` inside it, with the destructive border once the user has edited an invalid value (`:user-invalid`) or the server marked it (`aria-invalid`, which `e.aria(field)` writes); `Select` styles the open list where the browser has `appearance: base-select` | |
| `Checkbox`, `Switch` | a `<label>` with the `<input type="checkbox">` inside and its text after; a switch is `role="switch"` on the checkbox, so it posts as one | |
| `RadioGroup`, `Radio` | a `<fieldset>` with its `<legend>`, and each `<input type="radio">` with its text | |
| `Separator` | an `<hr>` | `Orientation`: `Horizontal`, `Vertical` |
| `Skeleton` | a placeholder's shape, hidden from screen readers, still under `prefers-reduced-motion` | `SkeletonShape`: `Line`, `Block`, `Circle` |
| `Table` | the look of the `<table>` inside it, scrolling sideways when it is wider than the page | |
| `Breadcrumb` | a `<nav>` of typed links to the pages above (`Crumb { label, link: &url::Sets }`), then the page, `aria-current="page"` | |
| `Pagination` | a `<nav>` of links to a list's pages, each from `link(n)`, a typed route; the first, the last and two each side of the current page, Previous and Next disabled at the ends | |
| `Avatar` | a person's picture over their initials, which show while it loads and when there is none | |
| `Dialog`, `DialogFooter` | the look of the `<dialog>` inside it, its `::backdrop` and its actions; a button opens it with `command="show-modal" commandfor={..}` and closes it with `command="close"`, with no script | |
| `Sheet` | the same, sliding in from an edge of the window | `Side`: `Top`, `Right`, `Bottom`, `Left` |
| `Popover` | the look of the `popover` inside it, opened by `popovertarget`, under its button where the browser has anchor positioning | |
| `Accordion` | the look of the `<details name>` inside it, which open one at a time, and find-in-page opens the one with the match | |
| `Tabs`, `DropdownMenu`, `Combobox`, `Toasts` | the look of corten-ui's `TabGroup`, `MenuButton`, `ComboBox` and `Toaster` inside them, by role and state (`[role="tab"][aria-selected="true"]`, `[role="menu"]:popover-open`, `[role="option"][data-highlighted]`); `Toasts` places the toaster at the bottom right. The app depends on `corten-ui` and lists `corten_ui::COMPONENTS`, which `corten ui add` says | |

Variants are Rust enums matched to the stylesheet's classes, so a variant without a rule does
not compile, and every prop is written where the component is used, as for any component
([views](views.md)). The classes are static text chosen by a `match`: a page pays for joining two
or three of them, not for any lookup. No component has a script: each works without
JavaScript and under the CSP, with no `style=`, and the labels they show (Previous, Next, a
legend) are the app's, in its language.

## Size

The kit's CSS goes into `components.css` with the app's own, one cached file. Measured on a new
app with every component added (`corten ui add --all`) and nothing of its own but the starter's
counter: 26,566 bytes, 4,104 with brotli at level 11 and 4,673 with gzip at level 9, the theme's
tokens included. An app pays for the components it adds, and `corten ui add` adds one at a time.
