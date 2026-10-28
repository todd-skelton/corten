# Views

[Back to the guides](../../README.md#documentation)

`html!` reads like JSX and compiles to `push_str` calls: static markup is merged at compile
time, expressions are escaped as they are written, and the output buffer is sized from the last
render of the same call site.

| Write | Means |
|---|---|
| `"Price"` | text (source whitespace between tags is never output) |
| `{expr}` | an expression, escaped: anything that implements `Render` (strings, numbers, `Option`, other views) or else `Display`. `raw(html)` prints trusted HTML. Not inside `<script>` or `<style>`, which do not read escaped text back: there it does not compile |
| `href={url}`, `href={("/items/", &id)}` | an attribute; a tuple prints its parts without building a string |
| `hidden={flag}`, `class={maybe}` | `bool` prints the bare name or nothing; `None` leaves the attribute out |
| `<input {..e.aria(field)}>` | spreads `Attrs` |
| `for`, `if`/`else`, `match`, `let` | as in Rust, with markup in the braces; patterns too (`if let Shape::Circle { r } = shape`) |
| `<Card title="x" compact>children</Card>` | a component: a struct with those fields (`compact` is `true`), children in `children` |
| `<>...</>`, `<!DOCTYPE html>`, `<input>` | fragments, the doctype, void elements without a closing tag |

`#[component]` turns a function into a props struct and a `Render` impl, so the same
component renders inside a page and on its own as a patch, and the two cannot drift. A view owns
what it captures and renders once, so it can be returned from a function.

## The document

A page is a whole HTML document, built from components like everything else: the app's layout
component with the page in it. `corten new` writes the layout in `src/views.rs`, and it is the
app's to change; Corten adds nothing around it.

```rust
use corten::prelude::*;

#[component]
fn Layout(req: &Req, title: &str, head: impl Render, children: impl Render) -> impl Document {
    html! {
        <!DOCTYPE html>
        <html lang={req.lang()}>
            <head>
                <meta charset="utf-8">
                <link rel="stylesheet" href={req.asset("style.css")}>
                <title>{title}" | Shop"</title>
                {head}
                <Corten />
            </head>
            <body>
                <header>"Shop"</header>
                <main>{children}</main>
            </body>
        </html>
    }
}

async fn about(req: Req) -> HtmlResult {
    let head = html! { <meta name="description" content="Who we are"> };
    req.page(html! { <Layout req={&req} title="About" head={head}><h1>"About"</h1></Layout> })
}
```

`html!` that starts with `<html>` (or `<!DOCTYPE html>`) is a `Document`, and so is a component
that returns `impl Document`; `req.page`, `req.screen` and the error page take one. The
markup is checked when it compiles: one `<head>` with one `<title>` and one `<Corten />`, and
one `<main>`, not inside a `for`, `if` or `match`. Each part renders for the requests that use it:

| Part | Full load | Client navigation | Data client (`req.screen`) |
|---|---|---|---|
| `<title>`'s text | yes | yes | yes, as `"title"` |
| an expression or component in `<head>` (`{head}`): the page's own tags | yes | yes, each marked `data-corten-head` so the next page's replace them | no |
| `<Corten />`: the runtime, the import map, the page's scripts and stylesheets | yes | the area stylesheets the page needs | no |
| `<main>`'s children: the page | yes | yes | no |
| everything else: the header, the footer, the layout's own tags | yes | no: the runtime keeps what the page has | no |

So the layout costs a navigation nothing, and what the layout shows that a navigation cannot
update (the signed-in user, from a `Caller` the handler took) is covered by the page key: when it
changes, the runtime loads the next page in full. Keep a layout's props to what most pages set (the title,
their head tags); a page that needs a different document uses another layout. Each layout's
`html!` document has a key of its own (from where it is written), so a link from a page in one
layout to a page in another loads it in full, with no `data-reload`. A prop that changes the
layout around `<main>` (a promo bar on some pages only) is not seen by a navigation: make that a
second layout.

A nav link is written from one mention of its route, `<a {..req.nav(&url::News)}>"News"</a>`:
`href`, `data-nav` and `aria-current="page"` when it is the page (`req.nav_section` for a link
that stays current on the pages under it). The full load renders `aria-current`, and the runtime
moves it after each client navigation, so the header is right without rendering again. A tag
that writes `href`, `data-nav` or `aria-current` itself beside the spread does not compile, nor
does `data-nav` written by hand.

Structured data for search engines is a head tag too: `head={JsonLd { value: &product }}`
(feature `json`) writes a `ToJson` value in `<script type="application/ld+json">` with `<` as
`\u003c`, so a description that says `</script>` cannot end it. An expression written inside
`<script>` in `html!` does not compile, because it would be escaped as HTML.

`html!` that is one component is that component: its props are evaluated where the markup is,
so `req={&req}` borrows the handler's `req`. A prop that borrows something the function made
for the view is made inside the markup, where the view owns it:

```rust,ignore
fn job_dialog(job: &Job) -> impl Render + '_ {
    html! {
        let title = format!("{} rows", job.rows);   // owned by the view
        <Dialog title={&title}>{job.summary}</Dialog>
    }
}
```

With the `let` outside the `html!`, the view would borrow a local of a function that has
returned (E0515). `let`s and a layout component are a document, like the layout alone, and `let`s and a patch
component are a patch.
## Typed links and messages

The strings that usually drift between files are Rust types, checked when the app compiles.

```rust,ignore
#[derive(Route)]
#[route("/items/{id}")]
pub struct ItemRoute<'a> { pub id: &'a str, pub tab: Option<&'a str> }   // tab is a query parameter

corten::i18n::messages! {
    pub struct Msg { "en" => "locales/en.json", "fr" => "locales/fr.json" }
}

// in the app's #[corten::routes] module, each route names its handler:
// #[route("/items/{id}", get = show)] pub struct ItemRoute<'a> { .. }

#[derive(Id)]
pub struct OfferId<'a>(pub &'a str);                                          // offer-{key}

let t: Msg = req.t();
html! {
    <a href={ItemRoute { id: &item.id, tab: None }}>{item.title}</a>       // /items/a%20b
    <p>{t.item_show_all_offers(item.offers.len())}</p>
    <li id={OfferId(&offer.id)}>{offer.seller}</li>                         // a patch's target
}
req.patch(html! { {Remove(OfferId(&sold.id))} })                             // the same id
```

| What | Fails to compile when |
|---|---|
| `#[derive(Route)]` | the path names a parameter the struct lacks, a parameter is not a whole segment, a link leaves a field out |
| `messages!` | a locale lacks a key or has an extra one, a message takes different arguments in another locale, a call names a missing key or leaves out an argument, or passes one that is not text, an integer or a value `t` formats (a float goes through `t.number`) |
| `#[derive(UrlValue)]` | the enum has a variant with fields, a variant serde does not read (`skip_deserializing`), or a serde attribute that changes its words in a way it does not follow |
| `#[derive(Id)]` | `Remove` is given text instead of an id type; two ids in one program have the same name (each is a linker symbol); the struct holds more than one key |

An id is the type's name without `Id`, in kebab case (`OfferId` is `offer-`, then the key; a
unit struct, `StatusId`, is just `status`). Use one for every element a patch replaces or
removes, and for `for`, `popovertarget` or `commandfor` across components. Its name is a linker
symbol, so two copies of one library in a program (two versions) that both define an id do
not link.

Routes write their URL straight into the page (path parameters percent-encoded, `None` query
parameters left out). A query field is anything `Display`; an enum in an address (a tab, a sort,
what a redirect says happened) derives `UrlValue` beside `Deserialize`, which writes each
variant as the word serde reads (`#[serde(rename_all = "kebab-case")]`: `AwaitingLabel` is
`awaiting-label`), so the link and the handler cannot disagree. Messages are split into text and arguments when the crate compiles: one
without arguments is a `&'static str`, one with arguments writes its pieces into the page,
escaped, with no lookup and no intermediate string. Editing a catalog rebuilds the crate.

A message the page cannot name in its source, because another service answers with its key
(`{"titleKey": "queue.item.title", "titleParams": {..}}`), is `t.by_key(key, &args)`: the
message with that catalog key (dotted, as in the JSON) in the request's locale, its arguments
taken by name from `args` (pairs, a map or a JSON object). It is `None` when the catalog has no
such key or `args` lacks one of its arguments (an `Option` renders nothing when `None`), so the
page chooses what to show instead. Prefer the methods wherever the page knows the message.

Numbers, money and dates are written by `t` in the request's locale too, on the server, so a
price or a time is right on the first paint with no script:

```rust,ignore
const USD: Currency = Currency::new("USD");
html! {
    <td>{t.money(Minor(line.cents), USD)}</td>                           // $1,234.50, 1 234,50 $US
    <td>{t.money(Decimal::parse(&api.amount).unwrap_or_default(), USD)}</td> // "21.50" from an API
    <td>{t.number(listing.count)}</td>                                     // 1,234
    <td>{t.percent(Decimal::new(fee_bps, 4), 2)}</td>                      // 2.9%, 2,9 %, %2,9
    <p>{t.item_pending(t.date_time(at, DateStyle::Medium, TimeStyle::Short))}</p> // Mar 22, 2026, 12:00 PM UTC
    <p>{t.account_member_since(t.date(joined, MonthStyle::Long))}</p>     // Member since March 2025
}
```

`t.percent(ratio, digits)` takes the ratio (0.029 for 2.9%, so 290 basis points are
`Decimal::new(290, 4)`) and writes at most `digits` fraction digits, rounded half away from zero
and without trailing zeros, with the locale's sign where it puts it. `t.money` takes `Minor` units (cents) or a `Decimal` in the major unit, never a float and never
a bare integer (it could be either, so it does not compile), with the digits the currency is
shown with (0 for JPY, 3 for KWD). A price a person types into a form is read back the same way:
`t.parse_money(&input.price, USD)` is `Some(Minor(129950))` for `$1,299.50` in en and for
`1 299,50 $US` or `1299,5` in fr (with or without the symbol, the code, the groups or the
fraction, and without the direction marks some locales write), and `None` for a sign, a group of
the wrong size, a grouped number that starts with `0` (`0,500`), another locale's decimal
separator, more fraction digits than the currency has or an amount over `u64`. Every amount
every locale writes reads back, so a form never reads money
as a float or with a parser of its own. `MonthStyle` writes the month and year without the day, in
the locale's order and words (`Mar 2026`, `März 2026`); it has no time, so `t.date_time` does
not take one. A date is in UTC unless `.zone(Offset::hours(-5))` or a zone
of your own (`corten::i18n::Zone`, for one with daylight saving time) says otherwise, so a page
reads the same wherever it is rendered; `parse_timestamp` reads the RFC 3339 text APIs send.
For dates where the visitor is ("your listings come back on Nov 2"), the app sets
`App::client_zone()` and a handler takes `Option<ClientZone>`: the browser's IANA name
(`America/Chicago`), which the runtime keeps in a cookie from the first page it loads, so the
form posts and pages after it carry it. `None` is a first load or a browser without
JavaScript. The app's time zone library (`jiff`, `chrono-tz`) turns the name into its `Zone`
(a newtype that implements `corten::i18n::Zone`) and a picked date into the instant it means
there, with no script per form. A handler that takes it answers privately (`Cache-Control:
private`), so a `Public` one cannot; the layout around `<main>` is the same in every zone, so
reading it does not make the runtime load the next page in full.
Each listed locale's separators, currency symbols and date patterns are CLDR's, checked against
Intl, and only the locales `messages!` lists are in the app. A script that writes a number, an
amount or a date takes the page's format (`t.js().money(USD)`) and writes the same text
([Scripts](scripts.md#numbers-money-and-dates-a-script-writes)).
