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

## A route with no screen

Webhooks, OAuth callbacks, calls from other services and public APIs answer only JSON. Mount
them under a prefix; their paths stay whole, so route structs still work:

```rust
#[derive(Route)]
#[route("/api")]
pub struct ApiRoot;

#[derive(Route)]
#[route("/api/items/{id}/price")]
pub struct PriceApi<'a> { pub id: &'a str }

async fn price(Path(id): Path<String>) -> corten::Result<Json<Price>> {
    let price = load(&id).await?.ok_or_else(|| Error::new(format!("no item {id}")).with_status(StatusCode::NOT_FOUND))?;
    Ok(Json(price))
}

App::new(pages)
    .api::<ApiRoot>(|api| api.route::<PriceApi>(get(price)))   // .allow_origin(..) in the chain for a partner site
```

The prefix and every route are route types, so a route outside the prefix does not compile.
A handler that takes `State` gets it on its method router: `get(price).with_state(db)`.

`Json` is the prelude's `corten::Json`: axum's extractor, and a response written in one buffer
(faster than `axum::Json`; keep the prelude's unless a crate hands you axum's). The response type
derives `ToJson` next to `Serialize` (`#[derive(Serialize, ToJson)] struct Price ..`); it reads the
same `#[serde(...)]` attributes, and one it cannot write as serde does is a compile error.
Every failure under the prefix is `{"error":{"status","message"}}` whatever the client accepts;
a 4xx message is sent, so write it for the client. Validation is `Errors` as the error. There
is no shell and no `App::context` (add `.context()` to the API's chain if it serves your own
signed-in users). Do not add an API route for a screen: a native client reads the page with
`req.screen`.

## Routes only some callers may use

Put the routes in an area with a guard instead of checking in each handler; Corten answers in
each request mode (303 to sign in, `corten-location` for a patch, JSON 401 for data and API
routes, the error page for a refusal), and a refused request never reaches the handler:

```rust
App::new(routes)
    .context(|headers| viewer_from(headers))   // first: the app becomes an App<Viewer>
    .area::<Admin>(|admin| {
        admin.route::<Admin>(get(dashboard)).route::<AdminUser>(get(user).post(save_user)).guard(|viewer, req| match viewer {
            None => Access::sign_in(&SignIn),
            Some(v) if v.can(Permission::View) && (!req.writes() || v.can(Permission::Edit)) => Access::ALLOW,
            Some(_) => Access::refuse(Error::status(StatusCode::FORBIDDEN)),
        })
    })
```

The area's prefix (`Admin`, a plain path, no parameters) and each of its routes are route types:
a route outside the prefix, or a prefix with a parameter, does not build. The guard reads the
context's fields without naming its type. A prefix covers whole segments. `req.writes()` is every method but GET, HEAD and OPTIONS, so reads and writes at one
URL get different rules; `req.is::<Export>()` names one route. `App::guard(|viewer, req| ..)`
guards the whole app; open the sign-in page and webhooks with `.unguarded::<SignIn>()`. An API
takes `.guard(..)` in its chain. A check that needs the record (only its author may edit it)
stays in the handler as `Error::status(StatusCode::FORBIDDEN)`.

## Data from another service

A page built from another service's JSON deserializes the response into structs holding only
the fields the page reads, not a `serde_json::Value`. serde skips the other fields without
building them; for the example item page's 55 KB response that is a third of the work.
`#[serde(default)]` keeps a missing field from failing the page; a field that may be null is an
`Option`, and one the service sends as either a string or a number can stay a `Value`:

```rust
#[derive(Deserialize, Default)]
#[serde(default)]
struct ApiListing { listing_id: String, price_amount: serde_json::Value }

let listings: Vec<ApiListing> = client.get(url).send().await?.error_for_status()?.json().await?;
```

Convert those into the view model once, where the data is loaded, so views read plain fields.

## Caching

Everything the app renders is `private, no-cache` unless the handler says otherwise; static
files and the runtime are cached forever by their versioned URLs. A page or JSON that is the same
for every visitor may be shared by a CDN:

```rust
async fn home(req: Req) -> corten::Result<(Cache, Response)> {
    let cache = Cache::public(Duration::from_secs(60)).stale_while_revalidate(Duration::from_secs(600));
    Ok((cache, req.page("Home", view)?))
}
```

`App` still sends it `private, no-cache` when the request has a cookie or `Authorization`, the
context found someone, the response sets a cookie or the shell took `doc.nonce()`; a client
navigation or a data request gets `private` with the same lifetime. It cannot see a cookie your
own extractor reads, so never mark such a handler public. `Cache::no_store()` is for one-time
secrets. Do not build a server-side page cache: cache the data the page reads instead.
