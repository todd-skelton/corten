//! Typed routes: a struct per URL, so a link cannot point at a route that does not exist or miss
//! one of its parameters.
//!
//! ```ignore
//! #[derive(Route)]
//! #[route("/items/{id}")]
//! pub struct ItemRoute<'a> { pub id: &'a str, pub tab: Option<&'a str> }
//!
//! // #[corten::routes]: #[route("/items/{id}", get = show)] pub struct ItemRoute<'a> { .. }
//! html! { <a href={ItemRoute { id: &item.id, tab: None }}>{item.title}</a> }
//! req.redirect(&ItemRoute { id, tab: Some("offers") })            // "/items/x?tab=offers"
//! ```
//!
//! Fields named in the path fill it, percent-encoded (`{*rest}` keeps its slashes); every other
//! field is a query parameter, left out when it is `None`; a list (`Vec` or a slice) is the
//! parameter once per item, `?condition=nm&condition=lp`, and left out when empty (what
//! [`crate::Query`] reads back into a `Vec`); a map marked `#[route(prefix = "dim.")]` is a parameter
//! per entry, `?dim.form=graded` ([`Pairs`]). Field types only need `Display`, so a
//! link borrows (`&str`) and copies nothing. The derive checks the path when it compiles: a
//! parameter without a field, a parameter that is not a whole segment, or a character that would
//! need escaping is an error. Renaming the route's path changes every link with it.
//!
//! Handlers read parameters with axum's `Path` and Corten's [`crate::Query`]; a route struct with owned
//! fields that also derives `Deserialize` can be the extractor's type. A route struct says where a
//! link goes; that a handler is registered at `PATH` is the router's job (and its tests').
//!
//! A query parameter is named as serde reads the field: `#[serde(rename_all = "camelCase")]` on the
//! struct and `#[serde(rename = "..")]` on a field rename it in links too, so addresses that
//! already exist (`?priceMin=`) keep working and the link and the handler cannot disagree:
//!
//! ```
//! use corten::prelude::*;
//!
//! #[derive(Route, serde::Deserialize)]
//! #[route("/search")]
//! #[serde(rename_all = "camelCase")]
//! struct Search {
//!     price_min: Option<u32>,
//!     in_stock: Option<bool>,
//!     #[serde(rename = "q")]
//!     text: Option<String>,
//! }
//!
//! let link = Search { price_min: Some(5), in_stock: Some(true), text: Some("pika".into()) };
//! assert_eq!(link.href(), "/search?priceMin=5&inStock=true&q=pika");
//! ```
//!
//! A path parameter keeps the name the path gives it, so a field serde reads by another name does
//! not compile, and neither does a name a URL would have to escape:
//!
//! ```compile_fail
//! use corten::prelude::*;
//!
//! #[derive(Route, serde::Deserialize)]
//! #[route("/items/{item_id}")]
//! #[serde(rename_all = "camelCase")]
//! struct Item { item_id: String }
//! ```
//!
//! ```compile_fail
//! use corten::prelude::*;
//!
//! #[derive(Route, serde::Deserialize)]
//! #[route("/search")]
//! struct Search { #[serde(rename = "price min")] price_min: Option<u32> }
//! ```
//!
//! ```
//! use corten::prelude::*;
//!
//! #[derive(Route)]
//! #[route("/items/{id}")]
//! struct ItemRoute<'a> { id: &'a str, tab: Option<&'a str> }
//!
//! let link = html! { <a href={ItemRoute { id: "a b", tab: Some("offers") }}>"Item"</a> };
//! assert_eq!(link.to_html(), r#"<a href="/items/a%20b?tab=offers">Item</a>"#);
//! ```
//!
//! A path parameter the struct has no field for does not compile:
//!
//! ```compile_fail
//! use corten::prelude::*;
//!
//! #[derive(Route)]
//! #[route("/items/{id}")]
//! struct ItemRoute<'a> { slug: &'a str }
//! ```
//!
//! Nor does a link that leaves a parameter out, or a redirect to a path typed as text:
//!
//! ```compile_fail
//! fn done(req: corten::Req) -> corten::HtmlResult {
//!     req.redirect("/done")
//! }
//! ```
//!
//!
//! ```compile_fail
//! use corten::prelude::*;
//!
//! #[derive(Route)]
//! #[route("/items/{id}")]
//! struct ItemRoute<'a> { id: &'a str }
//!
//! let link = html! { <a href={ItemRoute {}}>"Item"</a> };
//! ```

use std::fmt;

/// Derives [`Route`](trait@Route) for a struct: `#[derive(Route)] #[route("/items/{id}")]`.
pub use corten_macros::Route;

/// Derives `Display` for an enum of unit variants as the words serde reads for them, so a route
/// field of that type (a tab, a sort, a filter, what a redirect says happened) writes the word its
/// handler reads back, and the words are written once, in `#[serde(rename_all)]` and
/// `#[serde(rename)]`:
///
/// ```
/// use corten::prelude::*;
///
/// #[derive(Clone, Copy, UrlValue, serde::Deserialize)]
/// #[serde(rename_all = "kebab-case")]
/// enum Tab { Open, AwaitingLabel, #[serde(rename = "done")] Shipped }
///
/// #[derive(Route, serde::Deserialize)]
/// #[route("/orders")]
/// struct Orders { tab: Option<Tab> }
///
/// assert_eq!(Orders { tab: Some(Tab::AwaitingLabel) }.href(), "/orders?tab=awaiting-label");
/// assert_eq!(Orders { tab: Some(Tab::Shipped) }.href(), "/orders?tab=done");
/// ```
///
/// A variant with fields is not one word, and one serde does not read would make a link its
/// handler refuses; neither compiles:
///
/// ```compile_fail
/// #[derive(corten::UrlValue, serde::Deserialize)]
/// enum Sort { Price { descending: bool } }
/// ```
///
/// ```compile_fail
/// #[derive(corten::UrlValue, serde::Deserialize)]
/// enum Tab { Open, #[serde(skip_deserializing)] Draft }
/// ```
///
/// Nor does an attribute that changes the words in a way it does not follow:
///
/// ```compile_fail
/// #[derive(corten::UrlValue, serde::Deserialize)]
/// #[serde(rename_all(deserialize = "kebab-case"))]
/// enum Tab { Open, AwaitingLabel }
/// ```
pub use corten_macros::UrlValue;

/// The app's routes in one module, each with the handlers that serve it.
///
/// Each `#[route("/..", get = show)]` struct in the module is a typed route (it gets
/// `#[derive(Route)]`) served by the handlers it names, one per method (`get`, `post`, `put`,
/// `delete`, `patch`, `head`, `options`, `trace`). Handlers are functions, named from where
/// the module is declared, so the module needs no `use super::*`. `router()` puts each one at its
/// route's path, for [`crate::App::new`]:
///
/// ```
/// use corten::prelude::*;
///
/// #[corten::routes]
/// mod url {
///     #[route("/", get = home)]
///     pub struct Home;
///
///     #[route("/items/{id}", get = item, post = save)]
///     pub struct Item<'a> {
///         pub id: &'a str,
///     }
/// }
///
/// async fn home() -> &'static str {
///     "home"
/// }
/// async fn item(Path(id): Path<String>) -> String {
///     id
/// }
/// async fn save(Path(id): Path<String>) -> String {
///     format!("saved {id}")
/// }
///
/// let app = App::new(url::router());
/// # let _ = app.router();
/// assert_eq!(url::Item { id: "a b" }.to_string(), "/items/a%20b");
/// ```
///
/// Handlers that share something (a database pool, a client) take it as `State`: the module
/// names its type, `#[corten::routes(state = Db)]`, and `router` takes it,
/// `App::new(url::router(db))`, so it cannot be forgotten. An area's routes and an API's go in a
/// module of their own, `#[corten::routes(area = Admin)]` or `#[corten::routes(api = ApiRoot)]`,
/// whose `area` or `api` function [`crate::App::area`] or [`crate::App::api`] takes; an API's
/// prefix route is the one route that may name no handler:
///
/// ```
/// use corten::prelude::*;
///
/// #[corten::routes(api = ApiRoot)]
/// mod api {
///     #[route("/api")]
///     pub struct ApiRoot;
///
///     #[route("/api/items/{id}", get = item)]
///     pub struct Item<'a> {
///         pub id: &'a str,
///     }
/// }
///
/// async fn item(Path(id): Path<String>) -> String {
///     format!("item {id}")
/// }
///
/// let app = App::new(Routes::new()).api(api::api);
/// # let _ = app.router();
/// ```
///
/// Any other route without a handler does not compile:
///
/// ```compile_fail
/// use corten::prelude::*;
///
/// #[corten::routes(api = ApiRoot)]
/// mod api {
///     #[route("/api")]
///     pub struct ApiRoot;
///
///     #[route("/api/items/{id}")]
///     pub struct Item<'a> {
///         pub id: &'a str,
///     }
/// }
/// ```
///
/// nor does a route outside the API's prefix:
///
/// ```compile_fail
/// use corten::prelude::*;
///
/// #[corten::routes(api = ApiRoot)]
/// mod api {
///     #[route("/api")]
///     pub struct ApiRoot;
///
///     #[route("/apis/items", get = items)]
///     pub struct Items;
/// }
///
/// async fn items() -> &'static str {
///     "items"
/// }
/// ```
///
/// and a module's routes are in an area or an API, not both:
///
/// ```compile_fail
/// use corten::prelude::*;
///
/// #[corten::routes(area = Admin, api = Admin)]
/// mod admin {
///     #[route("/admin", get = dashboard)]
///     pub struct Admin;
/// }
///
/// async fn dashboard() -> &'static str {
///     "admin"
/// }
/// ```
///
/// What each handler returns names what the route answers ([`crate::answer`]): `HtmlResult`,
/// `JsonResult<T>`, text, or `Public<..>` of one; a `Response` or `impl IntoResponse`, which
/// hides it, does not compile.
///
/// ```
/// use corten::prelude::*;
///
/// #[derive(Clone)]
/// struct Db;
///
/// #[corten::routes(state = Db)]
/// mod url {
///     #[route("/", get = home)]
///     pub struct Home;
/// }
///
/// async fn home(State(_db): State<Db>) -> &'static str {
///     "home"
/// }
///
/// let app = App::new(url::router(Db));
/// # let _ = app.router();
/// ```
///
/// A route without a handler does not compile:
///
/// ```compile_fail
/// #[corten::routes]
/// mod url {
///     #[route("/about")]
///     pub struct About;
/// }
/// ```
///
/// Nor does a handler whose arguments a request cannot fill:
///
/// ```compile_fail,E0277
/// use corten::prelude::*;
///
/// #[corten::routes]
/// mod url {
///     #[route("/", get = home)]
///     pub struct Home;
/// }
///
/// async fn home(_: u32) -> &'static str {
///     "home"
/// }
/// # fn main() { let _ = url::router(); }
/// ```
///
/// Nor two routes a request could not tell apart, which axum would refuse when the app starts:
///
/// ```compile_fail
/// # async fn item() {}
/// #[corten::routes]
/// mod url {
///     #[route("/items/{id}", get = item)]
///     pub struct Item<'a> { pub id: &'a str }
///
///     #[route("/items/{slug}", get = item)]
///     pub struct ItemBySlug<'a> { pub slug: &'a str }
/// }
/// ```
///
/// Nor a struct that derives `Route` itself in the module, the way a link-only route used to be
/// written:
///
/// ```compile_fail
/// #[corten::routes]
/// mod url {
///     #[derive(corten::Route)]
///     #[route("/admin")]
///     pub struct Admin;
/// }
/// ```
///
/// The routes of an [`crate::App::area`] are a module of their own, `#[corten::routes(area =
/// Admin)]`, where `Admin` is the route at the area's prefix. Its `area(area)` (`area(area,
/// state)` with `state = ..`) mounts each route in the area, with its stylesheet, so every route
/// the area's pages link to has a handler there too. `caller = Staff` holds every handler in the
/// module to taking `Caller<Staff>` ([`crate::Caller`]), so a route added to the area later cannot
/// forget who may use it:
///
/// ```
/// use corten::{axum::http::request::Parts, prelude::*};
///
/// #[derive(Clone, Hash)]
/// struct Staff;
///
/// impl<S: Sync> Identify<S> for Staff {
///     async fn identify(_parts: &Parts, _state: &S) -> Result<Self, Refused> {
///         Err(Refused::forbidden())
///     }
/// }
///
/// #[corten::routes(area = Admin, caller = Staff)]
/// mod admin {
///     #[route("/admin", get = dashboard)]
///     pub struct Admin;
///
///     #[route("/admin/users/{id}", get = user)]
///     pub struct User<'a> {
///         pub id: &'a str,
///     }
/// }
///
/// async fn dashboard(_: Caller<Staff>) -> &'static str {
///     "admin"
/// }
/// async fn user(_: Caller<Staff>, Path(id): Path<String>) -> String {
///     id
/// }
///
/// let app = App::new(Routes::new()).area(admin::area);
/// # let _ = app.router();
/// ```
///
/// A handler there that does not take it does not compile:
///
/// ```compile_fail,E0277
/// # use corten::{axum::http::request::Parts, prelude::*};
/// # #[derive(Clone, Hash)]
/// # struct Staff;
/// # impl<S: Sync> Identify<S> for Staff {
/// #     async fn identify(_parts: &Parts, _state: &S) -> Result<Self, Refused> { Err(Refused::forbidden()) }
/// # }
/// #[corten::routes(area = Admin, caller = Staff)]
/// mod admin {
///     #[route("/admin", get = dashboard)]
///     pub struct Admin;
/// }
///
/// async fn dashboard() -> &'static str {
///     "open to anyone"
/// }
/// # fn main() { let _ = corten::App::new(Routes::new()).area(admin::area); }
/// ```
///
/// A route of the module outside the area's prefix does not compile:
///
/// ```compile_fail
/// # async fn page() {}
/// #[corten::routes(area = Admin)]
/// mod admin {
///     #[route("/admin", get = page)]
///     pub struct Admin;
///
///     #[route("/admins/list", get = page)]
///     pub struct Admins;
/// }
/// ```
pub use corten_macros::routes;

/// A URL built from a typed value. Implemented by `#[derive(Route)]`, together with `Render`,
/// `AttrValue` (so `href={route}` works) and `Display` (the plain URL).
pub trait Route {
    /// The path as axum matches it: `"/items/{id}"`.
    const PATH: &'static str;

    /// Writes the URL. With `html`, query separators are written as `&amp;`.
    fn write_href(&self, out: &mut String, html: bool);

    /// The URL as a string, for redirects, canonical links and tests.
    fn href(&self) -> String {
        let mut out = String::with_capacity(Self::PATH.len() + 16);
        self.write_href(&mut out, false);
        out
    }

    /// This URL with a place on its page: the element with `id`, a `#[derive(Id)]` type, the
    /// one its markup prints (`<li id={ReviewId(&review.id)}>`). A redirect after a write
    /// (`req.redirect(&account.at(ReviewId(&id)))`) or a link brings the browser back to it.
    /// The markup and the URL cannot spell the id differently, and the browser scrolls to it on
    /// a full load and after a client navigation alike.
    ///
    /// ```
    /// use corten::prelude::*;
    ///
    /// #[derive(Route)]
    /// #[route("/accounts/{slug}")]
    /// struct Account<'a> { slug: &'a str, reported: Option<&'a str> }
    ///
    /// #[derive(Id)]
    /// struct ReviewId<'a>(&'a str);
    ///
    /// let account = Account { slug: "ada", reported: Some("7") };
    /// assert_eq!(account.at(ReviewId("7")).href(), "/accounts/ada?reported=7#review-7");
    /// // Escaped once for the attribute; percent-encoded in the fragment, as the browser reads it.
    /// let link = html! { <a href={account.at(ReviewId("7 & 8"))}>"Review"</a> };
    /// assert_eq!(link.to_html(), r#"<a href="/accounts/ada?reported=7#review-7%20%26%208">Review</a>"#);
    /// ```
    ///
    /// A place is an element id, so text does not compile:
    ///
    /// ```compile_fail
    /// use corten::prelude::*;
    ///
    /// #[derive(Route)]
    /// #[route("/accounts")]
    /// struct Accounts;
    ///
    /// let _ = Accounts.at("feedback");
    /// ```
    fn at<I: crate::view::Id>(&self, id: I) -> At<'_, Self, I> {
        At { route: self, id }
    }
}

/// The path an area or an API is under (`#[corten::routes(area = Docs)]`'s `Docs`): every
/// [`Route`], for an area with a page at its prefix, and the prefix route of an area's module
/// that names no handler, which is a prefix and nothing more. Nothing is served at such a prefix,
/// so it is not a link:
///
/// ```compile_fail,E0599
/// use corten::prelude::*;
///
/// #[corten::routes(area = Docs)]
/// mod docs {
///     #[route("/docs")]
///     pub struct Docs;
///
///     #[route("/docs/{page}", get = doc)]
///     pub struct DocPage<'a> { pub page: &'a str }
/// }
///
/// async fn doc(req: Req) -> HtmlResult {
///     req.page(html! { <main>"A page"</main> })
/// }
///
/// fn main() {
///     let _ = docs::Docs.href(); // no page is at /docs
/// }
/// ```
///
/// Such a prefix has no fields, since no URL is built from it:
///
/// ```compile_fail
/// #[corten::routes(area = Docs)]
/// mod docs {
///     #[route("/docs")]
///     pub struct Docs { pub section: u32 }
/// }
/// # fn main() {}
/// ```
pub trait Prefix {
    /// The path, plain (no `{param}`): `"/docs"`.
    const PREFIX: &'static str;
}

impl<R: Route + ?Sized> Prefix for R {
    const PREFIX: &'static str = R::PATH;
}

/// A route and a place on its page, made by [`Route::at`]: what a redirect, `Req::moved`, a
/// current-page check and `href={..}` take, written `/path?query#id`.
pub struct At<'a, R: ?Sized, I> {
    route: &'a R,
    id: I,
}

impl<R: Route + ?Sized, I: crate::view::Id> At<'_, R, I> {
    fn write_href(&self, out: &mut String, html: bool) {
        self.route.write_href(out, html);
        out.push('#');
        write_fragment(&self.id, out);
    }

    /// The URL as a string.
    pub fn href(&self) -> String {
        let mut out = String::with_capacity(R::PATH.len() + 24);
        self.write_href(&mut out, false);
        out
    }
}

/// Writes `id` as a fragment: the id as the element's markup has it, percent-encoded, which the
/// browser decodes to find the element. `write_id` escapes for an attribute (`&amp;`), so an id
/// with a character the URL must encode is unescaped first; most ids (`review-7`) need neither.
fn write_fragment(id: &(impl crate::view::Id + ?Sized), out: &mut String) {
    let start = out.len();
    id.write_id(out);
    if out.as_bytes()[start..].iter().all(|&b| KEEP[b as usize] & SEGMENT != 0) {
        return;
    }
    let escaped = out.split_off(start);
    let text =
        escaped.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&amp;", "&");
    Encode { out, keep: SEGMENT }.push(&text);
}

impl<R: Route + ?Sized, I: crate::view::Id> Href for At<'_, R, I> {
    fn with_href<T>(&self, f: impl FnOnce(&str) -> T) -> T {
        f(&self.href())
    }
}

impl<R: Route + ?Sized, I: crate::view::Id> crate::view::Render for At<'_, R, I> {
    fn render(&self, out: &mut String) {
        self.write_href(out, true)
    }
}

impl<R: Route + ?Sized, I: crate::view::Id> crate::view::AttrValue for At<'_, R, I> {
    fn write_attr(&self, name: &str, out: &mut String) {
        out.push_str(name);
        out.push_str("=\"");
        self.write_href(out, true);
        out.push('"');
    }
}

impl<R: Route + ?Sized, I: crate::view::Id> fmt::Display for At<'_, R, I> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.href())
    }
}

/// Query parameters that several routes carry: a list's filters, which its live rows, its bulk
/// preview and its confirm need too. Declared once with `#[derive(Route)] #[route(query)]`, and
/// included in a route with `#[route(query)]` on a field, which writes each of them as the route's
/// own parameters. A route that must carry them cannot leave one out, and making one route's URL
/// from another's copies one value. Handlers read them as `Query<ItemQuery>`.
///
/// ```
/// use corten::prelude::*;
///
/// #[derive(Clone, Default, serde::Deserialize, Route, Fields)]
/// #[route(query)]
/// struct ItemQuery { search: Option<String>, page: Option<u32> }
///
/// #[derive(Route)]
/// #[route("/items")]
/// struct Items { #[route(query)] filters: ItemQuery }
///
/// #[derive(Route)]
/// #[route("/items/live")]
/// struct ItemsLive { #[route(query)] filters: ItemQuery, since: Option<u64> }
///
/// let filters = ItemQuery { search: Some("red & blue".into()), page: Some(2) };
/// assert_eq!(Items { filters: filters.clone() }.href(), "/items?search=red%20%26%20blue&page=2");
/// assert_eq!(ItemsLive { filters, since: Some(7) }.href(), "/items/live?search=red%20%26%20blue&page=2&since=7");
/// assert_eq!(Items { filters: ItemQuery::default() }.href(), "/items");
///
/// // The handler reads the same struct back: `Query(filters): Query<ItemQuery>`.
/// let read: ItemQuery = corten::form::from_bytes(b"search=red%20%26%20blue&page=2").unwrap();
/// assert_eq!((read.search.as_deref(), read.page), (Some("red & blue"), Some(2)));
/// ```
///
/// A `#[route(query)]` field whose type is not a set of query parameters does not compile:
///
/// ```compile_fail
/// use corten::prelude::*;
///
/// #[derive(Route)]
/// #[route("/items")]
/// struct Items { #[route(query)] search: Option<String> }
/// ```
pub trait Params {
    /// Writes each parameter as `?key=value` or `&key=value` (`&amp;` with `html`), `first`
    /// saying which. Implemented by `#[derive(Route)] #[route(query)]`.
    fn write_params(&self, first: &mut bool, html: bool, out: &mut String);
}

/// Query parameters whose names are data: a filter per attribute, an option per product
/// dimension, where the keys come from the data, not the code. A field marked
/// `#[route(prefix = "dim.")]` writes each entry as `dim.<key>=<value>`, in order, the key and
/// the value percent-encoded; an empty one writes nothing. A `BTreeMap`, a `Vec` or slice of
/// pairs, or a reference to one; not a `HashMap`, whose order would make the same link differ
/// from one render to the next.
///
/// ```
/// use std::collections::BTreeMap;
/// use corten::prelude::*;
///
/// #[derive(Route)]
/// #[route("/items/{slug}")]
/// struct Item<'a> {
///     slug: &'a str,
///     #[route(prefix = "dimension.")]
///     dimensions: &'a BTreeMap<String, String>,
/// }
///
/// let dimensions = BTreeMap::from([("form".to_string(), "graded".to_string()), ("grade".to_string(), "PSA 8".to_string())]);
/// let link = Item { slug: "pika", dimensions: &dimensions };
/// assert_eq!(link.href(), "/items/pika?dimension.form=graded&dimension.grade=PSA%208");
///
/// // The handler reads them back with the other parameters, and keeps the prefixed ones.
/// #[derive(serde::Deserialize)]
/// struct ItemQuery {
///     #[serde(flatten)]
///     rest: BTreeMap<String, String>,
/// }
/// let read: ItemQuery = corten::form::from_bytes(b"dimension.form=graded&dimension.grade=PSA%208").unwrap();
/// let read: BTreeMap<String, String> =
///     read.rest.into_iter().filter_map(|(k, v)| Some((k.strip_prefix("dimension.")?.to_string(), v))).collect();
/// assert_eq!(read, dimensions);
/// ```
///
/// A prefix another parameter's name starts with would read that parameter as one of the family,
/// so it does not compile; nor does a prefix with a character a URL would have to escape:
///
/// ```compile_fail
/// use std::collections::BTreeMap;
/// use corten::prelude::*;
///
/// #[derive(Route)]
/// #[route("/items")]
/// struct Items<'a> {
///     #[route(prefix = "dim")]
///     dimensions: &'a BTreeMap<String, String>,
///     dimmed: Option<bool>,
/// }
/// ```
///
/// ```compile_fail
/// use std::collections::BTreeMap;
/// use corten::prelude::*;
///
/// #[derive(Route)]
/// #[route("/items")]
/// struct Items<'a> {
///     #[route(prefix = "dim[")]
///     dimensions: &'a BTreeMap<String, String>,
/// }
/// ```
///
/// And a `HashMap` is not one:
///
/// ```compile_fail
/// use std::collections::HashMap;
/// use corten::prelude::*;
///
/// #[derive(Route)]
/// #[route("/items")]
/// struct Items<'a> {
///     #[route(prefix = "dim.")]
///     dimensions: &'a HashMap<String, String>,
/// }
/// ```
#[diagnostic::on_unimplemented(
    message = "a #[route(prefix = ..)] field is a BTreeMap or a list of (key, value) pairs, not {Self}",
    note = "a HashMap's order changes from one render to the next, so the same link would differ: use a BTreeMap"
)]
pub trait Pairs {
    #[doc(hidden)]
    fn write_pairs(&self, prefix: &str, first: &mut bool, html: bool, out: &mut String);
}

impl<K: fmt::Display, V: fmt::Display> Pairs for std::collections::BTreeMap<K, V> {
    fn write_pairs(&self, prefix: &str, first: &mut bool, html: bool, out: &mut String) {
        for (key, value) in self {
            __private::keyed(prefix, key, value, first, html, out);
        }
    }
}

impl<K: fmt::Display, V: fmt::Display> Pairs for [(K, V)] {
    fn write_pairs(&self, prefix: &str, first: &mut bool, html: bool, out: &mut String) {
        for (key, value) in self {
            __private::keyed(prefix, key, value, first, html, out);
        }
    }
}

impl<K: fmt::Display, V: fmt::Display> Pairs for Vec<(K, V)> {
    fn write_pairs(&self, prefix: &str, first: &mut bool, html: bool, out: &mut String) {
        self.as_slice().write_pairs(prefix, first, html, out)
    }
}

impl<T: Pairs + ?Sized> Pairs for &T {
    fn write_pairs(&self, prefix: &str, first: &mut bool, html: bool, out: &mut String) {
        (**self).write_pairs(prefix, first, html, out)
    }
}

/// Where a redirect or a current-page check points: a typed route, a place on its page
/// ([`Route::at`]), or an `axum::http::Uri` for a URL the app does not route (a payment page, a
/// sign-in provider). Text is none of them, so a path typed as a string does not compile. Used by
/// [`crate::Req::redirect`] and [`crate::Req::current`].
pub trait Href {
    /// Calls `f` with the URL.
    fn with_href<T>(&self, f: impl FnOnce(&str) -> T) -> T;
}

impl Href for axum::http::Uri {
    fn with_href<T>(&self, f: impl FnOnce(&str) -> T) -> T {
        f(&self.to_string())
    }
}

/// A place on a page outside the app: an address the app does not route (another site, another
/// app of the same product) and the id of an element there, which that page's markup owns. An
/// `axum::http::Uri` cannot hold a `#fragment` (it drops one it parses), so this is what a
/// redirect to such a place takes:
///
/// ```
/// use corten::{axum::http::Uri, prelude::*, route::Elsewhere};
///
/// let compare = Uri::from_static("https://example.com/compare?x=1");
/// let to = Elsewhere::at(&compare, "fee-calculator");
/// // In a handler: req.moved(&to), req.redirect(&to).
/// assert_eq!(to.href(), "https://example.com/compare?x=1#fee-calculator");
/// // The id as the element has it, percent-encoded as the browser decodes it.
/// assert_eq!(Elsewhere::at(&compare, "fees & taxes").href(), "https://example.com/compare?x=1#fees%20&%20taxes");
/// ```
///
/// A place on one of the app's own pages is [`Route::at`], whose id the markup and the URL share,
/// so the address is a full URL (a scheme and a host); a path is a mistake a debug build reports.
pub struct Elsewhere<'a> {
    uri: &'a axum::http::Uri,
    id: &'a str,
}

impl<'a> Elsewhere<'a> {
    /// The element with `id` on the page at `uri`.
    pub fn at(uri: &'a axum::http::Uri, id: &'a str) -> Self {
        debug_assert!(
            uri.scheme().is_some() && uri.authority().is_some(),
            "corten: Elsewhere::at takes a full URL outside the app ({uri}); a place on the app's own page is route.at(Id)"
        );
        Elsewhere { uri, id }
    }

    /// The URL as a string: the address, `#`, and the id.
    pub fn href(&self) -> String {
        let mut out = self.uri.to_string();
        out.push('#');
        Encode { out: &mut out, keep: FRAGMENT }.push(self.id);
        out
    }
}

impl Href for Elsewhere<'_> {
    fn with_href<T>(&self, f: impl FnOnce(&str) -> T) -> T {
        f(&self.href())
    }
}

impl<R: Route> Href for R {
    fn with_href<T>(&self, f: impl FnOnce(&str) -> T) -> T {
        f(&self.href())
    }
}

/// For each byte, `SEGMENT` when it is written as it is in a path segment or query value
/// (unreserved: letters, digits, `-._~`), `REST` when it is in a `{*rest}` value (those and
/// `/`), and `FRAGMENT` when it is in another site's `#fragment` (those, `/`, `?`, `:`, `@` and
/// the sub-delimiters `!$&'()*+,;=`); everything else is %XX.
static KEEP: [u8; 256] = {
    let mut table = [0u8; 256];
    let mut b = 0;
    while b < 256 {
        let c = b as u8;
        if c.is_ascii_alphanumeric() || matches!(c, b'-' | b'.' | b'_' | b'~') {
            table[b] = SEGMENT | REST | FRAGMENT;
        }
        if matches!(c, b'/' | b'?' | b':' | b'@' | b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'=') {
            table[b] = FRAGMENT;
        }
        b += 1;
    }
    table[b'/' as usize] = REST | FRAGMENT;
    table
};
const SEGMENT: u8 = 1;
const REST: u8 = 2;
const FRAGMENT: u8 = 4;

/// Percent-encodes what is written to it into `out`. `keep` is `SEGMENT`, `REST` or `FRAGMENT`.
struct Encode<'a> {
    out: &'a mut String,
    keep: u8,
}

impl Encode<'_> {
    /// A value with nothing to encode (most ids and slugs) is one copy, found by ANDing every
    /// byte's table entry, with no branch per byte. Otherwise each run of bytes kept as they are is
    /// copied at once. A run is ASCII; between two encoded bytes of one character it is empty, and
    /// is skipped there, since that index is not a character boundary.
    #[inline]
    fn push(&mut self, s: &str) {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        let bytes = s.as_bytes();
        if bytes.iter().fold(self.keep, |all, &b| all & KEEP[b as usize]) != 0 {
            self.out.push_str(s);
            return;
        }
        let mut start = 0;
        while let Some(at) = bytes[start..].iter().position(|&b| KEEP[b as usize] & self.keep == 0) {
            let i = start + at;
            let b = bytes[i];
            if at > 0 {
                self.out.push_str(&s[start..i]);
            }
            self.out.push('%');
            self.out.push(HEX[(b >> 4) as usize] as char);
            self.out.push(HEX[(b & 15) as usize] as char);
            start = i + 1;
        }
        self.out.push_str(&s[start..]);
    }
}

impl fmt::Write for Encode<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.push(s);
        Ok(())
    }
}

/// A route's handlers, one per method: `get(show)`, `get(show).post(save)`. What
/// [`crate::Area::route`] and [`crate::Api::route`] take, and what `#[corten::routes]` builds for
/// each route it names. Each handler is checked when the crate compiles, as a routes module checks
/// it: its arguments are extractors a request fills, and its result is an [`Answer`](crate::Answer)
/// (`HtmlResult`, `JsonResult<T>`, `Public<..>` of one, ...), so a handler that hides what it
/// answers does not compile:
///
/// ```compile_fail,E0277
/// use corten::{axum::response::Response, prelude::*};
///
/// async fn rates() -> Response {
///     Response::default()
/// }
/// let _ = get(rates);
/// ```
///
/// and neither does a `Public` answer whose handler may read who is asking:
///
/// ```compile_fail,E0277
/// use std::time::Duration;
/// use corten::{axum::http::HeaderMap, prelude::*};
///
/// async fn rates(headers: HeaderMap) -> Public<&'static str> {
///     let _ = headers.get("x-api-key");
///     Public::new(Duration::from_secs(60), "rates")
/// }
/// let _ = get(rates);
/// ```
///
/// Nor does a method router made by axum's own `get`, which checks nothing:
///
/// ```compile_fail,E0308
/// use corten::prelude::*;
///
/// #[derive(Route)]
/// #[route("/admin")]
/// struct Admin;
///
/// async fn dashboard() -> &'static str {
///     "admin"
/// }
/// let app = App::new(Routes::new()).area::<Admin>(|admin| admin.route::<Admin>(corten::axum::routing::get(dashboard)));
/// ```
///
/// `S` is the state its handlers take (`State<S>`), given with [`Methods::with_state`].
pub struct Methods<S = ()>(pub(crate) axum::routing::MethodRouter<S>);

impl<S: Clone + Send + Sync + 'static> Methods<S> {
    /// The handlers with the state they take (`State<S>`), for a router without it:
    /// `get(item_json).with_state(source)`.
    pub fn with_state<S2>(self, state: S) -> Methods<S2> {
        Methods(self.0.with_state(state))
    }

    /// Used by `#[corten::routes]`: a route's handlers for two sets of methods. Not a public API.
    #[doc(hidden)]
    pub fn __merge(self, other: Methods<S>) -> Self {
        Methods(self.0.merge(other.0))
    }
}

macro_rules! methods {
    ($($method:ident),*) => {
        $(
            #[doc = concat!("The handler for `", stringify!($method), "` requests to a route, checked when the crate compiles; see [`Methods`].")]
            #[inline]
            pub fn $method<H, T, S>(handler: H) -> Methods<S>
            where
                H: __private::RouteHandler<T, S> + crate::answer::Signature<T>,
                <H as __private::RouteHandler<T, S>>::Handler: axum::handler::Handler<T, S>,
                <<H as __private::RouteHandler<T, S>>::Handler as axum::handler::Handler<T, S>>::Future: Unpin,
                <H as crate::answer::Signature<T>>::Out: crate::answer::Answer + 'static,
                <<H as crate::answer::Signature<T>>::Out as crate::answer::Answer>::Inputs:
                    crate::answer::Accepts<<H as crate::answer::Signature<T>>::Args>,
                T: 'static,
                S: Clone + Send + Sync + 'static,
            {
                Methods(axum::routing::$method(__private::endpoint(handler)))
            }
        )*

        impl<S: Clone + Send + Sync + 'static> Methods<S> {
            $(
                #[doc = concat!("This route's handler for `", stringify!($method), "` requests too, checked as [`", stringify!($method), "()`] checks it.")]
                #[inline]
                pub fn $method<H, T>(self, handler: H) -> Self
                where
                    H: __private::RouteHandler<T, S> + crate::answer::Signature<T>,
                    <H as __private::RouteHandler<T, S>>::Handler: axum::handler::Handler<T, S>,
                    <<H as __private::RouteHandler<T, S>>::Handler as axum::handler::Handler<T, S>>::Future: Unpin,
                    <H as crate::answer::Signature<T>>::Out: crate::answer::Answer + 'static,
                    <<H as crate::answer::Signature<T>>::Out as crate::answer::Answer>::Inputs:
                        crate::answer::Accepts<<H as crate::answer::Signature<T>>::Args>,
                    T: 'static,
                {
                    Methods(self.0.$method(__private::endpoint(handler)))
                }
            )*
        }
    };
}
methods!(get, post, put, delete, patch, head, options, trace);

/// The app's routes, each at its path with its handlers: what [`crate::App::new`] takes, made by a
/// `#[corten::routes]` module's `router()`. An app whose every route is under
/// [`crate::App::api`] or [`crate::App::area`] starts from `Routes::new()`, which has none.
///
/// It is the only thing `App::new` takes, so no route reaches the app without its handler checked;
/// an axum `Router` does not compile there:
///
/// ```compile_fail,E0308
/// use corten::prelude::*;
///
/// async fn home() -> &'static str {
///     "home"
/// }
/// let app = App::new(corten::axum::Router::new().route("/", corten::axum::routing::get(home)));
/// ```
#[derive(Default)]
pub struct Routes(pub(crate) axum::Router);

impl Routes {
    /// No routes.
    pub fn new() -> Self {
        Routes::default()
    }

    /// Used by `#[corten::routes]`: `R`'s handlers at its path. Not a public API.
    #[doc(hidden)]
    pub fn __route<R: Route>(self, methods: Methods) -> Self {
        Routes(self.0.route(R::PATH, methods.0))
    }
}

/// The crate's tests build routers by hand, to test what `App` does with any response.
#[cfg(test)]
impl From<axum::Router> for Routes {
    fn from(router: axum::Router) -> Self {
        Routes(router)
    }
}

/// Used by the code `#[derive(Route)]` generates. Not a public API.
#[doc(hidden)]
pub mod __private {
    use std::{
        borrow::Cow,
        fmt::{self, Display, Write as _},
    };

    use axum::handler::Handler;

    use super::{Encode, Methods, REST, SEGMENT};

    // `#[corten::routes]` names each handler through one of these, so a handler that cannot serve
    // the route is an error that says why, at the handler's name in `#[route(..)]`: its arguments
    // a request cannot fill, a result that is not an `Answer`, an input a `Public` answer may not
    // read, or a module's `caller = ..` it does not take. `R` is that caller (`()` for none).
    // `super::get` and the others are the same check without a caller.
    macro_rules! methods {
        ($($method:ident),*) => {
            $(
                #[inline]
                pub fn $method<H, T, S, R, I>(handler: H) -> Methods<S>
                where
                    H: RouteHandler<T, S> + crate::answer::Signature<T>,
                    <H as RouteHandler<T, S>>::Handler: Handler<T, S>,
                    <<H as RouteHandler<T, S>>::Handler as Handler<T, S>>::Future: Unpin,
                    <H as crate::answer::Signature<T>>::Out: crate::answer::Answer + 'static,
                    <<H as crate::answer::Signature<T>>::Out as crate::answer::Answer>::Inputs:
                        crate::answer::Accepts<<H as crate::answer::Signature<T>>::Args>,
                    <H as crate::answer::Signature<T>>::Args: crate::answer::Holds<R, I>,
                    T: 'static,
                    S: Clone + Send + Sync + 'static,
                {
                    Methods(axum::routing::$method(endpoint(handler)))
                }
            )*

        };
    }
    methods!(get, post, put, delete, patch, head, options, trace);

    /// The handler with its answer in its type, so a JSON route's failures are marked as JSON.
    #[inline]
    pub(crate) fn endpoint<H, T, S>(handler: H) -> crate::answer::Endpoint<H::Handler, <H as crate::answer::Signature<T>>::Out>
    where
        H: RouteHandler<T, S> + crate::answer::Signature<T>,
    {
        crate::answer::Endpoint { handler: handler.into_handler(), answer: std::marker::PhantomData }
    }

    /// A handler a route names: an async function whose arguments a request fills (extractors,
    /// `Req`, `State<..>` of the routes module's state) and whose result is a response.
    #[diagnostic::on_unimplemented(
        message = "`{Self}` cannot serve this route",
        label = "not a handler for these routes",
        note = "a handler is an async fn whose arguments are extractors (Req, Path, Query, Form, ..), with a State<T> only for the module's #[corten::routes(state = T)], and whose result is a response"
    )]
    pub trait RouteHandler<T, S> {
        type Handler: Handler<T, S>;
        fn into_handler(self) -> Self::Handler;
    }

    impl<H: Handler<T, S>, T, S> RouteHandler<T, S> for H {
        type Handler = H;
        #[inline(always)]
        fn into_handler(self) -> H {
            self
        }
    }

    // A path parameter: one segment, percent-encoded (a `/` in the value is `%2F`), or with
    // `rest` the rest of the path (`{*rest}`), slashes kept. A text field is encoded straight from
    // the string; any other `Display` field through its `Display` output. The derive calls
    // `(&Segment(&field)).__segment(..)`, so the text impls, which take `Segment` itself, are
    // found before the one for `&Segment`.
    pub struct Segment<'a, T: ?Sized>(pub &'a T);

    #[inline]
    fn keep(rest: bool) -> u8 {
        if rest {
            REST
        } else {
            SEGMENT
        }
    }

    pub trait SegmentText {
        fn __segment(&self, rest: bool, out: &mut String);
    }

    impl SegmentText for Segment<'_, &str> {
        #[inline]
        fn __segment(&self, rest: bool, out: &mut String) {
            Encode { out, keep: keep(rest) }.push(self.0)
        }
    }

    impl SegmentText for Segment<'_, String> {
        #[inline]
        fn __segment(&self, rest: bool, out: &mut String) {
            Encode { out, keep: keep(rest) }.push(self.0)
        }
    }

    impl SegmentText for Segment<'_, Cow<'_, str>> {
        #[inline]
        fn __segment(&self, rest: bool, out: &mut String) {
            Encode { out, keep: keep(rest) }.push(self.0)
        }
    }

    pub trait SegmentValue {
        fn __segment(&self, rest: bool, out: &mut String);
    }

    impl<T: Display + ?Sized> SegmentValue for &Segment<'_, T> {
        #[inline]
        fn __segment(&self, rest: bool, out: &mut String) {
            let _ = write!(Encode { out, keep: keep(rest) }, "{}", self.0);
        }
    }

    fn separator(first: &mut bool, html: bool, out: &mut String) {
        out.push_str(match (*first, html) {
            (true, _) => "?",
            (false, true) => "&amp;",
            (false, false) => "&",
        });
        *first = false;
    }

    fn pair(key: &str, value: &dyn Display, first: &mut bool, html: bool, out: &mut String) {
        separator(first, html, out);
        out.push_str(key);
        let _ = fmt::write(&mut Encode { out, keep: SEGMENT }, format_args!("{value}"));
    }

    /// `{prefix}{key}={value}`, the key and value encoded (the prefix is checked when it compiles).
    pub(super) fn keyed(prefix: &str, key: &dyn Display, value: &dyn Display, first: &mut bool, html: bool, out: &mut String) {
        separator(first, html, out);
        out.push_str(prefix);
        let _ = fmt::write(&mut Encode { out, keep: SEGMENT }, format_args!("{key}"));
        out.push('=');
        let _ = fmt::write(&mut Encode { out, keep: SEGMENT }, format_args!("{value}"));
    }

    // `Option` fields are left out when `None`, lists write one pair per item; any other `Display`
    // field is always written.
    pub struct Query<'a, T: ?Sized>(pub &'a T);

    pub trait QueryOption {
        fn __query(&self, key: &str, first: &mut bool, html: bool, out: &mut String);
    }

    impl<T: Display> QueryOption for Query<'_, Option<T>> {
        #[inline]
        fn __query(&self, key: &str, first: &mut bool, html: bool, out: &mut String) {
            if let Some(value) = self.0 {
                pair(key, value, first, html, out)
            }
        }
    }

    impl<T: Display> QueryOption for Query<'_, Vec<T>> {
        #[inline]
        fn __query(&self, key: &str, first: &mut bool, html: bool, out: &mut String) {
            for value in self.0 {
                pair(key, value, first, html, out)
            }
        }
    }

    impl<T: Display> QueryOption for Query<'_, &[T]> {
        #[inline]
        fn __query(&self, key: &str, first: &mut bool, html: bool, out: &mut String) {
            for value in *self.0 {
                pair(key, value, first, html, out)
            }
        }
    }

    pub trait QueryValue {
        fn __query(&self, key: &str, first: &mut bool, html: bool, out: &mut String);
    }

    impl<T: Display + ?Sized> QueryValue for &Query<'_, T> {
        #[inline]
        fn __query(&self, key: &str, first: &mut bool, html: bool, out: &mut String) {
            pair(key, &self.0, first, html, out)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{self as corten, route::Route, view::Render};

    #[derive(corten::Route)]
    #[route("/")]
    struct Home;

    #[derive(corten::Route)]
    #[route("/items/{id}/offers")]
    struct Offers<'a> {
        id: &'a str,
        page: Option<u32>,
        sort: &'a str,
    }

    #[derive(corten::Route)]
    #[route("/files/{*path}")]
    struct File {
        path: String,
    }

    #[test]
    fn routes_fill_their_path_and_query() {
        assert_eq!(Home::PATH, "/");
        assert_eq!(Home.href(), "/");
        assert_eq!(Offers::PATH, "/items/{id}/offers");
        let offers = Offers { id: "a b/c?", page: Some(2), sort: "price&desc" };
        assert_eq!(offers.href(), "/items/a%20b%2Fc%3F/offers?page=2&sort=price%26desc");
        assert_eq!(offers.to_html(), "/items/a%20b%2Fc%3F/offers?page=2&amp;sort=price%26desc");
        assert_eq!(Offers { id: "x", page: None, sort: "new" }.href(), "/items/x/offers?sort=new");
        assert_eq!(File { path: "a/b c.txt".into() }.to_string(), "/files/a/b%20c.txt");
        let link = crate::html! { <a href={Offers { id: "x", page: Some(1), sort: "new" }}>"x"</a> }.to_html();
        assert_eq!(link, r#"<a href="/items/x/offers?page=1&amp;sort=new">x</a>"#);
    }

    #[derive(corten::Route)]
    #[route("/n/{n}/{name}/{label}")]
    struct Mixed<'a> {
        n: u32,
        name: std::borrow::Cow<'a, str>,
        label: &'a String,
    }

    macro_rules! words {
        ($($name:ident = $rule:literal),*) => {$(
            #[derive(Clone, Copy, PartialEq, Debug, corten::UrlValue, serde::Deserialize)]
            #[serde(rename_all = $rule)]
            #[allow(clippy::upper_case_acronyms)]
            enum $name {
                Open,
                AwaitingLabel,
                HTTPCode,
                XRay2,
                #[serde(rename = "kept-as-is")]
                Renamed,
                #[serde(alias = "old")]
                Aliased,
            }

            impl $name {
                const ALL: [Self; 6] = [Self::Open, Self::AwaitingLabel, Self::HTTPCode, Self::XRay2, Self::Renamed, Self::Aliased];
            }
        )*};
    }

    words!(
        Lower = "lowercase",
        Upper = "UPPERCASE",
        Pascal = "PascalCase",
        Camel = "camelCase",
        Snake = "snake_case",
        ScreamingSnake = "SCREAMING_SNAKE_CASE",
        Kebab = "kebab-case",
        ScreamingKebab = "SCREAMING-KEBAB-CASE"
    );

    #[derive(corten::UrlValue, serde::Deserialize, Clone, Copy, PartialEq, Debug)]
    enum Plain {
        Open,
        AwaitingLabel,
    }

    /// Each word `UrlValue` writes is the one serde reads back as the same variant, under every
    /// rule, through the reader `Query` uses.
    #[test]
    fn url_values_are_the_words_serde_reads() {
        fn round_trip<E: std::fmt::Display + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug + Copy>(
            all: &[E],
        ) -> Vec<String> {
            #[derive(serde::Deserialize)]
            struct Back<E> {
                v: E,
            }
            all.iter()
                .map(|value| {
                    let word = value.to_string();
                    let back: Back<E> = crate::form::from_bytes(format!("v={word}").as_bytes()).unwrap();
                    assert_eq!(back.v, *value, "{word}");
                    word
                })
                .collect()
        }
        assert_eq!(round_trip(&Kebab::ALL), ["open", "awaiting-label", "h-t-t-p-code", "x-ray2", "kept-as-is", "aliased"]);
        assert_eq!(round_trip(&Camel::ALL), ["open", "awaitingLabel", "hTTPCode", "xRay2", "kept-as-is", "aliased"]);
        round_trip(&Lower::ALL);
        round_trip(&Upper::ALL);
        round_trip(&Pascal::ALL);
        round_trip(&Snake::ALL);
        round_trip(&ScreamingSnake::ALL);
        round_trip(&ScreamingKebab::ALL);
        assert_eq!(round_trip(&[Plain::Open, Plain::AwaitingLabel]), ["Open", "AwaitingLabel"]);
    }

    #[test]
    fn text_and_display_parameters_encode_the_same() {
        let label = "~a.b-c_d é".to_string();
        let mixed = Mixed { n: 7, name: "é/x%".into(), label: &label };
        assert_eq!(mixed.href(), "/n/7/%C3%A9%2Fx%25/~a.b-c_d%20%C3%A9");
        assert_eq!(File { path: "/é//".into() }.href(), "/files//%C3%A9//");
    }

    #[derive(corten::Route, serde::Deserialize)]
    #[route("/search")]
    struct Search {
        q: Option<String>,
        #[serde(default)]
        condition: Vec<String>,
    }

    #[derive(corten::Route)]
    #[route("/compare")]
    struct Compare<'a> {
        ids: &'a [&'a str],
    }

    #[derive(corten::Route)]
    #[route("/items/{slug}")]
    struct Variant<'a> {
        slug: &'a str,
        sort: Option<&'a str>,
        #[route(prefix = "dimension.")]
        dimensions: &'a std::collections::BTreeMap<&'a str, &'a str>,
    }

    #[derive(corten::Route)]
    #[route("/items")]
    struct Pairs {
        #[route(prefix = "d.")]
        options: Vec<(String, u32)>,
    }

    #[test]
    fn prefixed_parameters_write_each_entry_in_order_encoded() {
        let dimensions = std::collections::BTreeMap::from([("grade", "PSA 8"), ("form", "a&b/c=d")]);
        let link = Variant { slug: "pika", sort: Some("new"), dimensions: &dimensions };
        assert_eq!(link.href(), "/items/pika?sort=new&dimension.form=a%26b%2Fc%3Dd&dimension.grade=PSA%208");
        assert_eq!(link.to_html(), "/items/pika?sort=new&amp;dimension.form=a%26b%2Fc%3Dd&amp;dimension.grade=PSA%208");
        let empty = std::collections::BTreeMap::new();
        assert_eq!(Variant { slug: "pika", sort: None, dimensions: &empty }.href(), "/items/pika");
        let keys = std::collections::BTreeMap::from([("a b", "x"), ("é=", "y")]);
        assert_eq!(
            Variant { slug: "p", sort: None, dimensions: &keys }.href(),
            "/items/p?dimension.a%20b=x&dimension.%C3%A9%3D=y"
        );
        let pairs = Pairs { options: vec![("z".into(), 2), ("a".into(), 1), ("z".into(), 3)] };
        assert_eq!(pairs.href(), "/items?d.z=2&d.a=1&d.z=3", "a list keeps its order and its repeats");
    }
    #[test]
    fn a_list_is_the_parameter_once_per_item_and_reads_back() {
        let search = Search { q: Some("charizard ex".into()), condition: vec!["nm".into(), "lp&mp".into()] };
        assert_eq!(search.href(), "/search?q=charizard%20ex&condition=nm&condition=lp%26mp");
        assert_eq!(search.to_html(), "/search?q=charizard%20ex&amp;condition=nm&amp;condition=lp%26mp");
        assert_eq!(Search { q: None, condition: vec![] }.href(), "/search");
        assert_eq!(Compare { ids: &["a", "b"] }.href(), "/compare?ids=a&ids=b");
        let query = search.href();
        let back: Search = crate::form::from_bytes(query.split_once('?').unwrap().1.as_bytes()).unwrap();
        assert_eq!((back.q.as_deref(), back.condition), (Some("charizard ex"), vec!["nm".to_string(), "lp&mp".to_string()]));
    }

    /// Every byte, alone and inside kept text, at every position of short and long values, encodes
    /// as RFC 3986 says: unreserved bytes (and `/` in a rest value) as they are, the rest %XX.
    #[test]
    fn every_byte_encodes_at_every_position() {
        use super::{Encode, REST, SEGMENT};
        fn reference(s: &str, rest: bool) -> String {
            s.bytes()
                .map(|b| {
                    if b.is_ascii_alphanumeric() || b"-._~".contains(&b) || (rest && b == b'/') {
                        (b as char).to_string()
                    } else {
                        format!("%{b:02X}")
                    }
                })
                .collect()
        }
        let mut values: Vec<String> = vec![String::new(), "é".into(), "日本語".into(), "a/é b".into()];
        for b in 0u8..128 {
            for len in [1, 2, 7, 8, 9, 17] {
                for at in [0, len / 2, len - 1] {
                    let mut v = vec![b'k'; len];
                    v[at] = b;
                    values.push(String::from_utf8(v).unwrap());
                }
            }
        }
        for v in &values {
            for (keep, rest) in [(SEGMENT, false), (REST, true)] {
                let mut out = String::new();
                Encode { out: &mut out, keep }.push(v);
                assert_eq!(out, reference(v, rest), "{v:?}");
            }
        }
    }
}
