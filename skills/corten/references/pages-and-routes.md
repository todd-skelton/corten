# Pages, routes, messages and data screens

## A page

```rust
use corten::prelude::*;

#[derive(Route)]
#[route("/items/{id}")]
pub struct Item<'a> {
    pub id: &'a str,
}

// Router::new().route(Item::PATH, get(show))
async fn show(req: Req, Path(id): Path<String>) -> corten::Result {
    // Any std::error::Error converts with `?` (a 500 and the error page). For a 404:
    // .ok_or_else(|| corten::Error::status(StatusCode::NOT_FOUND))?
    let item = load(&id).await?;
    req.page(item.title.as_str(), html! {
        <h1>{item.title}</h1>
        <a href={Item { id: &item.related }}>"Related"</a>
    })
}
```

1. Mount the route with `Item::PATH`; link with the struct. Fields not in the path are query
   parameters (`pub tab: Option<&'a str>`). Redirect with `req.redirect(&Item { id })`.
2. The first argument of `req.page` is the title, or a `Meta` for description, canonical and
   social tags.
3. The shell (`App::shell`) wraps every page; client navigation swaps only `<main>`. Mark the
   current link with `aria-current={doc.current(&Item { id })}` in the shell.
4. Read everything from `req` before building the view: `html!` views render once and own what
   they capture.

## A component

```rust
#[component]
pub fn Price(amount: &str) -> impl Render {
    html! { <td>{amount}</td> }   // classes come from the component's CSS file, see styles.md
}
// <Price amount={&row.price} />
```

Text in `{...}` and attribute values are escaped. `raw(..)` is the only way around escaping;
use it only for HTML the app produced itself.

## Messages

```rust
corten::i18n::messages! {
    pub struct Msg { "en" => "locales/en.json", "fr" => "locales/fr.json" }
}
// locales/en.json: { "item": { "show_all_offers": "Show all {count} offers" } }

let t: Msg = req.t();                         // the request's locale
html! { <button>{t.item_show_all_offers(offers.len())}</button> }
```

Every locale must have every key of the first with the same arguments, or the crate does not
compile. Pass `t: Msg` into components that show text.

## A data screen

A screen that native clients or agents also read derives `Serialize`, and its fields are the
whole view model, formatted text included:

```rust
#[derive(Serialize)]
pub struct Home { stocks: Vec<Stock>, message: String }

async fn home(req: Req, State(m): State<Market>) -> corten::Result {
    let home = Home::new(&m, String::new());
    req.screen("Watchlist", &home, home_view(&home))   // HTML, or {"title","data"} for Accept: application/json
}
```
