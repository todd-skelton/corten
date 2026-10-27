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

async fn show(req: Req, Path(id): Path<String>) -> HtmlResult {
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
   `#[corten::routes(state = Db)]` and `url::router(db)`. Link with the struct. Fields not in the path are query parameters (`pub tab: Option<&'a str>`). An enum field (`pub tab: Option<Tab>`) derives `UrlValue` with `Deserialize`, never a hand-written `Display`: it writes the words serde reads.
   Redirect with `req.redirect(&url::Item { id })`; a page that moved for good (an old slug)
   answers `req.moved(&url::Item { id })`, a 301. A place on another site's page is
   `corten::Elsewhere::at(&uri, "id")`, never a `Location` header set by hand. An area's routes are a module of their own,
   `#[corten::routes(area = Admin, caller = Staff)] mod admin` whose routes name their handlers
   the same way, mounted with `.area(admin::area)`; an API's are a `#[corten::routes(api = ApiRoot)]`
   module, `.api(api::api)`.
   A handler's return type is what the route answers: `HtmlResult` (a page, a patch, a
   redirect; failures are the app's error page), `JsonResult<T>` (failures are JSON), or
   `Public<..>` of either for an answer a CDN may share (`NoStore<..>` for one nothing may keep).
   `Response` or `impl IntoResponse` there does not compile. `App::new` takes only a routes
   module's `router()` (`Routes`), and `Area::route`/`Api::route` only Corten's `get(h)`,
   `post(h)`, ... (`Methods`), which check handlers the same way: never build an axum `Router`. Every
   route names its handler except an area's or API's prefix route, which may name none; an
   area's prefix with no handler is not a link (no `href`), so never declare a link-only struct
   outside the module for it.
2. `req.page` takes the whole document: the app's `Layout` (in `src/views.rs`, the app's code)
   with the page as its children. The title and the page's head tags (description, canonical,
   social) are the layout's props. Never build a document outside a layout component. A page
   showing a read that has not caught up with a write yet adds
   `ReadAgain { after: Duration::from_secs(2) }` to its head tags while it is behind (and stops
   after a time limit); the runtime reads it again in place however it was reached. A
   `data-patch` answer in the same state puts `{ReadAgain { after }.from(&route)}` in the patch,
   where `route` carries the write's receipt and its handler answers `req.patch(..)` in
   `Mode::Action`; never redirect only to show "saving". Never a `Refresh` header. Structured data is `JsonLd { value: &product }` among the head tags, never
   `raw` or an expression inside `<script>` (that does not compile).
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
built by hand. A parameter whose URL name is not the field's (`?priceMin=`) is renamed with serde
(`#[serde(rename_all = "camelCase")]` or `#[serde(rename = "..")]`), which the link follows; never
implement `Params` by hand for it.

## A component

```rust
#[component]
pub fn Price(amount: &str) -> impl Render {
    html! { <td>{amount}</td> }   // classes come from the component's CSS file, see styles.md
}
// <Price amount={&row.price} />
```

Text in `{...}` and attribute values are escaped. `raw(..)` is the only way around escaping;
use it only for HTML the app produced itself. A view that is one component and passes it
something it builds makes it in the markup: `html! { let title = format!(..); <Dialog title={&title} /> }`.

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

Numbers, money and dates come from `t` too, never `format!` or a `"%"` of your own: `t.number(n)`,
`t.percent(Decimal::new(bps, 4), 2)` (a ratio, at most 2 fraction digits), `t.money(Minor(cents),
USD)` or `t.money(Decimal::parse(&api.amount)?, USD)` (`const USD: Currency = Currency::new("USD")`;
never a float), `t.date(at, DateStyle::Medium)` (or `MonthStyle::Long` for `March 2026`), `t.time(at, TimeStyle::Short)` and
`t.date_time(at, DateStyle::Medium, TimeStyle::Short)` (UTC unless `.zone(..)`;
`corten::i18n::parse_timestamp` reads an API's RFC 3339 text). Each is a message argument too. A
script gets the same formats from `t.js()` (`references/scripts.md`).

## A data screen

A screen that native clients or agents also read derives `Serialize`, and its fields are the
whole view model, formatted text included:

```rust
#[derive(Serialize)]
pub struct Home { stocks: Vec<Stock>, message: String }

async fn home(req: Req, State(m): State<Market>) -> HtmlResult {
    let home = Home::new(&m, String::new());
    let view = home_view(&home);
    req.screen(&home, html! { <Layout req={&req} title="Watchlist" head="">{view}</Layout> })   // HTML, or {"title","data"}
}
```

## A route with no screen

Webhooks, OAuth callbacks, calls from other services and public APIs answer only JSON: their
handlers return `JsonResult<T>`, so every failure of the route is JSON wherever it is. Routes
that share a prefix and its settings (a partner site's CORS) go in a module of their own; their
paths stay whole, so route structs still work:

```rust
#[corten::routes(api = ApiRoot, state = Db)]
pub mod api {
    #[route("/api")]                                  // the prefix: the one route without a handler
    pub struct ApiRoot;

    #[route("/api/items/{id}/price", get = price)]
    pub struct PriceApi<'a> { pub id: &'a str }
}

async fn price(Path(id): Path<String>) -> JsonResult<Price> {
    let price = load(&id).await?.ok_or_else(|| Error::new(format!("no item {id}")).with_status(StatusCode::NOT_FOUND))?;
    Ok(Json(price))
}

App::new(pages)
    .api(|routes| api::api(routes, db))   // .allow_origin(..) in the chain for a partner site
```

A JSON handler `?`s its input's check as well: `check(&input)?` in a `JsonResult<T>` handler
answers 422 with the fields (its error is a `JsonError`, an `Error` or the `Errors`). A page
does not: there `?` on `Errors` does not compile, and the form answers with `req.invalid`.

A route outside the prefix, or one without a handler, does not compile. An endpoint built from a
value (the MCP endpoint) is added with `Api::route`: `mcp::api(routes).route::<mcp::Mcp>(tools.route())`.

`Json` is the prelude's `corten::Json`: axum's extractor, and a response written in one buffer
(faster than `axum::Json`; keep the prelude's unless a crate hands you axum's). The response type
derives `ToJson` next to `Serialize` (`#[derive(Serialize, ToJson)] struct Price ..`); it reads the
same `#[serde(...)]` attributes, and one it cannot write as serde does is a compile error.
Every failure under the prefix is `{"error":{"status","message"}}` whatever the client accepts;
a 4xx message is sent, so write it for the client. Validation is `Errors` as the error. There
is no document; a handler that needs to know who calls takes a `Caller` (an API key's partner
type), and a refusal is a JSON 401 or 403. An API that must answer in another protocol's error
shape sets `.errors(|status, message| MyError { .. })` (a `ToJson`) on it, so keep key checks in
`Identify` rather than in handlers. Do not add an API route for a screen: a native client reads the page with
`req.screen`.

A WebSocket is a route whose handler takes `corten::upgrade::Upgrade` and returns
`upgrade.websocket(|stream, signal| async move { .. })`; read the frames with a library
(`tokio_tungstenite::WebSocketStream::from_raw_socket(stream, Role::Server, config)`) and end on
`signal.aborted()`. Its handshake is checked as a write (another site's page gets a 403), so do
not add an `Origin` check of your own. A URL that is also a page or a relayed answer takes
`Option<Upgrade>` and returns `corten::Result<SwitchOr<A>>`. A route that passes another server's answer on returns
`corten::Relay::new(status, headers, body)`, never a `Response`; never merge a router after
`app.router()` for either.

A date the visitor picks, or a time shown where they are, needs their time zone: set
`App::client_zone()` and take `Option<ClientZone>` (`None` on a first load), then convert with the
app's time zone library. Never add a page script that converts dates.

## Routes only some callers may use

A handler that needs to know who is asking takes `Caller<T>`; `T`'s `Identify` finds the caller
(the session store, an API key) or refuses, and Corten answers in each request mode (303 to sign
in, `corten-location` for a patch, JSON 401 for data and JSON routes, the error page for a
refusal). The lookup runs only when a handler reads it, once per request, and makes the answer
private:

```rust
#[derive(Clone, Hash)]
pub struct Staff { pub id: u64, pub editor: bool }

impl Identify<Db> for Staff {
    async fn identify(parts: &Parts, db: &Db) -> Result<Self, Refused> {
        let Some(session) = cookie::get(&parts.headers, "session") else {
            return Err(Refused::sign_in(&url::SignIn { next: Some(parts.uri.path()) }));
        };
        match db.staff_for(session).await {
            Ok(Some(staff)) => Ok(staff),
            Ok(None) => Err(Refused::forbidden()),
            Err(error) => Err(Refused::unavailable(error)),   // the store is down: a 503, not sign-in
            // An API key: Err(Refused::unauthorized().message("Invalid API key")), and after too many
            // failures (the app counts them) Err(Refused::too_many(wait)): a 429 with Retry-After.
        }
    }
}

#[corten::routes(state = Db, area = Admin, caller = Staff)]
mod admin {
    #[route("/admin", get = dashboard)]
    pub struct Admin;

    #[route("/admin/users/{id}", get = user, post = save_user)]
    pub struct AdminUser<'a> { pub id: &'a str }
}

async fn save_user(req: Req, Caller(staff): Caller<Staff>, Form(input): Form<UserInput>) -> HtmlResult {
    if !staff.editor {
        return Err(Error::status(StatusCode::FORBIDDEN));
    }
    ..
}

App::new(url::router(db.clone())).area(|area| admin::area(area, db))
```

`caller = Staff` holds every handler in the module to taking `Caller<Staff>`: one added later
without it does not compile. The area's prefix (`Admin`, a plain path, no parameters) and each
of its routes are route types: a route outside the prefix, or a prefix with a parameter, does
not build. A page that shows something either way (a layout's "Sign in") takes
`Option<Caller<Viewer>>`. Reads and writes at one URL are separate handlers (`get = show,
post = save`), so each takes what it needs. A private app puts `caller = Staff` on its routes
module and the sign-in route in a module without it. A check that needs the record (only its
author may edit it) stays in the handler as `Error::status(StatusCode::FORBIDDEN)`.

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
for every visitor says so in its type, and a CDN may share it:

```rust
async fn home(req: Req) -> Public<HtmlResult> {
    let page = req.page(html! { <Layout req={&req} title="Home" head="">{view}</Layout> });
    Public::new(Duration::from_secs(60), page).stale_while_revalidate(Duration::from_secs(600))
}
```

Its handler may only take inputs that cannot tell who is asking (`Path`, `Query`, `Form`,
`State`, `Req`, ...): a `Caller`, a `HeaderMap` or a cookie there does not compile. `App` still
sends it `private, no-cache` when the request has a cookie or `Authorization`, the response sets
a cookie or the layout took `req.nonce()`; a client navigation or a data request gets `private`
with the same lifetime. `.shared(Duration)` gives shared caches a longer lifetime than the
browser's (`s-maxage`; a private answer drops it). Only a success or a redirect is shared. `NoStore<HtmlResult>` is for one-time
secrets. Do not build a server-side page cache: cache the data the page reads instead.
