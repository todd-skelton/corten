# Pages, routes, messages and data screens

## A page

```rust
use corten::prelude::*;

#[corten::routes]
pub mod url {
    #[route("/items/{id}", get = show)]
    pub struct Item<'a> {
        pub id: &'a str,
    }
}

async fn show(req: Req, Path(id): Path<String>) -> corten::Result {
    // Any std::error::Error converts with `?` (a 500 and the error page). For a 404:
    // .ok_or_else(|| corten::Error::status(StatusCode::NOT_FOUND))?
    let item = load(&id).await?;
    let head = html! { <meta name="description" content={&item.summary}> };
    req.page(html! {
        <Layout req={&req} title={&item.title} head={head}>
            <h1>{&item.title}</h1>
            <a href={url::Item { id: &item.related }}>"Related"</a>
        </Layout>
    })
}
```

1. Every route is a struct in the `#[corten::routes]` module that names its handlers,
   `#[route("/items/{id}", get = show, post = save)]` (a route without one does not compile);
   `App::new(url::router())`. Handlers that share something take it as `State<Db>`, with
   `#[corten::routes(state = Db)]` and `url::router(db)`. Link with the struct. Fields not in the path are query parameters (`pub tab: Option<&'a str>`).
   Redirect with `req.redirect(&url::Item { id })`; a page that moved for good (an old slug)
   answers `req.moved(&url::Item { id })`, a 301. A route an area or an API serves is a link
   only: `#[route("/admin", link)]`.
2. `req.page` takes the whole document: the app's `Layout` (in `src/views.rs`, the app's code)
   with the page as its children. The title and the page's head tags (description, canonical,
   social) are the layout's props. Never build a document outside a layout component. A page
   showing a read that has not caught up with a write yet adds
   `ReadAgain { after: Duration::from_secs(2) }` to its head tags while it is behind (and stops
   after a time limit); the runtime reads it again in place however it was reached. Never a
   `Refresh` header.
3. A client navigation gets only the `<title>`, the head tags and `<main>`'s children; the rest of
   the layout renders on full loads. Mark the current link with
   `aria-current={req.current(&url::Item { id })}` in the layout.
4. Read everything from `req` before building the view: `html!` views render once and own what
   they capture (`let t = req.t();` first, then `<Layout req={&req} ..>`).

Routes that carry the same query parameters (a list's filters on its live, bulk and confirm
routes) share one `#[derive(Route)] #[route(query)] struct ItemQuery { .. }` as a
`#[route(query)] filters: ItemQuery` field; handlers read `Query<ItemQuery>`. Never repeat the
filter fields in each route. Parameters whose names are data (an option per dimension) are a
`#[route(prefix = "dimension.")] dimensions: &BTreeMap<String, String>` field, never a query string
built by hand.

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

When an API answers with a message key and its arguments, show it with
`t.by_key(&row.title_key, &row.title_params)` (an `Option`: `None` for an unknown key or a missing
argument; choose the fallback). Never write a `match` from key strings to methods.

Numbers, money and dates come from `t` too, never `format!`: `t.number(n)`, `t.money(Minor(cents),
USD)` or `t.money(Decimal::parse(&api.amount)?, USD)` (`const USD: Currency = Currency::new("USD")`;
never a float), `t.date(at, DateStyle::Medium)`, `t.time(at, TimeStyle::Short)` and
`t.date_time(at, DateStyle::Medium, TimeStyle::Short)` (UTC unless `.zone(..)`;
`corten::i18n::parse_timestamp` reads an API's RFC 3339 text). Each is a message argument too. A
script gets the same formats from `t.js()` (`references/scripts.md`).

## A data screen

A screen that native clients or agents also read derives `Serialize`, and its fields are the
whole view model, formatted text included:

```rust
#[derive(Serialize)]
pub struct Home { stocks: Vec<Stock>, message: String }

async fn home(req: Req, State(m): State<Market>) -> corten::Result {
    let home = Home::new(&m, String::new());
    let view = home_view(&home);
    req.screen(&home, html! { <Layout req={&req} title="Watchlist" head="">{view}</Layout> })   // HTML, or {"title","data"}
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
is no document and no `App::context` (add `.context()` to the API's chain if it serves your own
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

A page built from another service's JSON reads the response into structs holding only the
fields the page reads, not a `serde_json::Value`, with `corten::json::from_slice` on a type that
derives `FromJson` next to `Deserialize`. The other fields are skipped without building them;
for the example item page's 55 KB response that is a quarter of the work of a `Value`, and a
fifth less than serde_json into the same structs. `#[serde(default)]` keeps a missing field
from failing the page; a field that may be null is an `Option`, and one the service sends as
either a string or a number can stay a `Value`:

```rust
#[derive(Deserialize, FromJson, Default)]
#[serde(default)]
struct ApiListing { listing_id: String, price_amount: serde_json::Value }

let body = client.get(url).send().await?.error_for_status()?.bytes().await?;
let listings: Vec<ApiListing> = corten::json::from_slice(&body)?;
```

Convert those into the view model once, where the data is loaded, so views read plain fields.

## Caching

Everything the app renders is `private, no-cache` unless the handler says otherwise; static
files and the runtime are cached forever by their versioned URLs. A page or JSON that is the same
for every visitor may be shared by a CDN:

```rust
async fn home(req: Req) -> corten::Result<(Cache, Response)> {
    let cache = Cache::public(Duration::from_secs(60)).stale_while_revalidate(Duration::from_secs(600));
    Ok((cache, req.page(html! { <Layout req={&req} title="Home" head="">{view}</Layout> })?))
}
```

`App` still sends it `private, no-cache` when the request has a cookie or `Authorization`, the
context found someone, the response sets a cookie or the layout took `req.nonce()`; a client
navigation or a data request gets `private` with the same lifetime. It cannot see a cookie your
own extractor reads, so never mark such a handler public. `Cache::no_store()` is for one-time
secrets. Do not build a server-side page cache: cache the data the page reads instead.
