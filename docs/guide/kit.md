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
```

| Command | Writes |
|---|---|
| `corten ui init` | `tokens/ui.tokens.json` (light), `tokens/ui-dark.tokens.json` and `tokens/ui.resolver.json` (the theme modifier, following the system when the page chose no theme), `src/ui.rs` (the tokens and the `COMPONENTS` list), the page's colors at the end of `static/style.css`; in `src/lib.rs`, `mod ui;` and `.components(ui::COMPONENTS)` on the `App::new(..)` line. It refuses to run twice, and says what to do by hand when `src/lib.rs` or `static/style.css` is not where `corten new` puts them |
| `corten ui add <name>..`, `--all` | each component's files in `src/ui/`, `pub mod <name>;` and the component in `src/ui.rs`'s `COMPONENTS`, laid out as rustfmt does. A file that exists is refused, before anything is written, unless `--force` |

Each file starts with the kit version it came from (`// corten ui 0.1.0: button`). The files
are embedded in the `corten` binary, so adding one needs no network, and a component is the
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
use crate::ui::{button::*, card::*, input::*};

<Card>
    <CardHeader><h2>"Sign in"</h2><p>"Use the email you registered with."</p></CardHeader>
    <CardContent>
        <Input><input type="email" name={f.email} required></Input>
    </CardContent>
    <CardFooter>
        <Button variant={ButtonVariant::Primary} size={ButtonSize::Md}><button>"Sign in"</button></Button>
        <Button variant={ButtonVariant::Link} size={ButtonSize::Md}><a href={url::Reset}>"Forgot it?"</a></Button>
    </CardFooter>
</Card>
```

| Component | Renders | Variants |
|---|---|---|
| `Button` | the look of the `<button>` or `<a>` inside it | `ButtonVariant`: `Primary`, `Secondary`, `Outline`, `Ghost`, `Destructive`, `Link`; `ButtonSize`: `Sm`, `Md`, `Lg`, `Icon` |
| `Badge` | a `<span>` label | `BadgeVariant`: `Primary`, `Secondary`, `Outline`, `Destructive` |
| `Card`, `CardHeader`, `CardContent`, `CardFooter` | a surface and its parts; the heading and description in the header are the app's (`<h2>` or `<h3>`, as the page's outline needs, then `<p>`) | |
| `Input` | the look of the `<input>` inside it, with the destructive border once the user has edited an invalid value (`:user-invalid`) or the server marked it (`aria-invalid`, which `e.aria(field)` writes) | |
| `Alert` | a `role="alert"` box, so a screen reader reads it when a patch brings it in | `AlertVariant`: `Neutral`, `Destructive` |

Variants are Rust enums matched to the stylesheet's classes, so a variant without a rule does
not compile, and every prop is written where the component is used, as for any component
([views](views.md)). The classes are static text chosen by a `match`: a page pays for joining two
or three of them, not for any lookup. No component has a script: each works without
JavaScript and under the CSP, with no `style=`.
