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
//! [`Params`] reads back into a `Vec`); a map marked `#[route(prefix = "dim.")]` is a parameter
//! per entry, `?dim.form=graded` ([`Pairs`]). Field types only need `Display`, so a
//! link borrows (`&str`) and copies nothing. The derive checks the path when it compiles: a
//! parameter without a field, text before a parameter in its segment, or a character that would
//! need escaping is an error. Renaming the route's path changes every link with it.
//!
//! A parameter may be followed by plain text in its segment, a file's extension: `{page}.xml`.
//! A request whose segment does not end in the text, or is only the text, is a 404 before the
//! handler runs, and [`Params`] reads the parameter without it, as its type:
//!
//! ```
//! use corten::prelude::*;
//!
//! #[derive(Route)]
//! #[route("/sitemap/{kind}/{page}.xml")]
//! struct SitemapPage<'a> { kind: &'a str, page: u32 }
//!
//! assert_eq!(SitemapPage { kind: "items", page: 3 }.href(), "/sitemap/items/3.xml");
//! ```
//!
//! The router matches such a segment whole, so two routes of one module that differ only in the
//! text after a parameter do not compile:
//!
//! ```compile_fail
//! use corten::Params;
//!
//! #[corten::routes]
//! mod url {
//!     #[route("/feeds/{name}.rss", get = rss)]
//!     pub struct Rss<'a> { pub name: &'a str }
//!
//!     #[route("/feeds/{name}.atom", get = atom)]
//!     pub struct Atom<'a> { pub name: &'a str }
//! }
//! async fn rss(_: Params<url::Rss<'_>>) -> &'static str { "" }
//! async fn atom(_: Params<url::Atom<'_>>) -> &'static str { "" }
//! # fn main() {}
//! ```
//!
//! Text goes after a parameter, in its segment, and only one parameter is in a segment; a
//! `{*rest}`, which takes the rest of the path, has nothing after it:
//!
//! ```compile_fail
//! #[derive(corten::Route)]
//! #[route("/files/{*path}.xml")]
//! struct File { path: String }
//! ```
//!
//! ```compile_fail
//! #[derive(corten::Route)]
//! #[route("/files/{name}.{ext}")]
//! struct File { name: String, ext: String }
//! ```
//!
//! ```compile_fail
//! #[derive(corten::Route)]
//! #[route("/items/item_{id}")]
//! struct Item { id: u32 }
//! ```
//!
//! Nor does a route with a parameter where another of the module has its catch-all, after the same
//! segments, which the router cannot hold (`/docs/{name}.md` can sit beside `/{*page}`, which has
//! its catch-all where the other has `docs`):
//!
//! ```compile_fail
//! use corten::Params;
//!
//! #[corten::routes]
//! mod url {
//!     #[route("/{name}.txt", get = text)]
//!     pub struct Text<'a> { pub name: &'a str }
//!
//!     #[route("/{*page}", get = page)]
//!     pub struct Page<'a> { pub page: &'a str }
//! }
//! async fn text(_: Params<url::Text<'_>>) -> &'static str { "" }
//! async fn page(_: Params<url::Page<'_>>) -> &'static str { "" }
//! # fn main() {}
//! ```
//!
//! ```
//! use corten::Params;
//!
//! #[corten::routes]
//! mod url {
//!     #[route("/docs/{name}.md", get = doc)]
//!     pub struct Doc<'a> { pub name: &'a str }
//!
//!     #[route("/{*page}", get = page)]
//!     pub struct Page<'a> { pub page: &'a str }
//! }
//! async fn doc(_: Params<url::Doc<'_>>) -> &'static str { "" }
//! async fn page(_: Params<url::Page<'_>>) -> &'static str { "" }
//! # fn main() { let _ = corten::App::new(url::router()).router(); }
//! ```
//!
//! (Either feed route alone compiles, as does `/files/{name}.xml`:)
//!
//! ```
//! #[derive(corten::Route)]
//! #[route("/files/{name}.xml")]
//! struct File { name: String }
//! ```
//!
//! ```
//! use corten::Params;
//!
//! #[corten::routes]
//! mod url {
//!     #[route("/feeds/{name}.rss", get = rss)]
//!     pub struct Rss<'a> { pub name: &'a str }
//! }
//! async fn rss(_: Params<url::Rss<'_>>) -> &'static str { "" }
//! # fn main() {}
//! ```
//!
//! A route's handler reads the same parameters back as [`Params`]: the struct's fields, owned, so a
//! link and its handler cannot disagree about a parameter. A route struct says where a link goes;
//! that a handler is registered at `PATH` is the router's job (and its tests').
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

/// Derives `Display` and `AsRef<str>` for an enum of unit variants as the words serde reads for
/// them, so a route field of that type (a tab, a sort, a filter, what a redirect says happened)
/// writes the word its handler reads back, and the words are written once, in
/// `#[serde(rename_all)]` and `#[serde(rename)]`. A form field of that type works the same way:
/// an `<option value={Condition::NearMint}>` (or corten-ui's `Choice`) posts the word its
/// handler's `condition: Condition` reads, and a form with any other word is refused (422), as one
/// with a value of the wrong kind is.
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
/// assert_eq!(Tab::Open.as_ref(), "open");
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
/// Nor do two variants written as one word, since a link to one would read back as the other:
///
/// ```compile_fail
/// #[derive(corten::UrlValue, serde::Deserialize)]
/// #[serde(rename_all = "lowercase")]
/// enum Plan { Ab, AB }
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
/// async fn item(item: Params<url::Item<'_>>) -> String {
///     item.id.clone()
/// }
/// async fn save(item: Params<url::Item<'_>>) -> String {
///     format!("saved {}", item.id)
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
/// async fn item(item: Params<api::Item<'_>>) -> String {
///     format!("item {}", item.id)
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
/// async fn user(_: Caller<Staff>, user: Params<admin::User<'_>>) -> String {
///     user.id.clone()
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
/// A page in the area that another caller may use is a module of its own in the same area, passed
/// in the same call (`.area(|area| audit::area(admin::area(area)))`); each module's handlers are
/// held to its own caller, so one there that takes the other module's does not compile:
///
/// ```compile_fail,E0631
/// # use corten::{axum::http::request::Parts, prelude::*};
/// # #[derive(Clone, Hash)]
/// # struct Staff;
/// # impl<S: Sync> Identify<S> for Staff {
/// #     async fn identify(_parts: &Parts, _state: &S) -> Result<Self, Refused> { Err(Refused::forbidden()) }
/// # }
/// # #[derive(Clone, Hash)]
/// # struct Auditor;
/// # impl<S: Sync> Identify<S> for Auditor {
/// #     async fn identify(_parts: &Parts, _state: &S) -> Result<Self, Refused> { Err(Refused::forbidden()) }
/// # }
/// #[corten::routes(area = Admin, caller = Staff)]
/// mod admin {
///     #[route("/admin", get = dashboard)]
///     pub struct Admin;
/// }
///
/// #[corten::routes(area = super::admin::Admin, caller = Auditor)]
/// mod audit {
///     #[route("/admin/audit", get = audit_log)]
///     pub struct AuditLog;
/// }
///
/// async fn dashboard(_: Caller<Staff>) -> &'static str {
///     "admin"
/// }
/// async fn audit_log(_: Caller<Staff>) -> &'static str {
///     "the staff's caller, in the auditors' module"
/// }
/// # fn main() { let _ = corten::App::new(Routes::new()).area(|area| audit::area(admin::area(area))); }
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
    /// The path as axum matches it: `"/items/{id}"`. A parameter followed by text in its segment
    /// (`#[route("/sitemap/{page}.xml")]`) is matched as the whole segment (`"/sitemap/{page}"`),
    /// and Corten checks the text.
    const PATH: &'static str;

    /// The path as written, `"/sitemap/{page}.xml"`, when a parameter is followed by text in its
    /// segment; otherwise [`Route::PATH`]. A request whose segment does not end in the text is a 404
    /// before the handler runs, and `Params` reads the parameter without it.
    #[doc(hidden)]
    const WRITTEN: &'static str = Self::PATH;

    /// Whether the runtime may fetch the page ahead of a click (on a hover): `false` for a route
    /// declared `#[route("/..", prefetch = false)]`, a page costly to draw, so a link to it
    /// (`href={..}`, a nav's `{..req.nav(..)}` and `req.nav_of`) is written with `data-no-prefetch`
    /// and only a click starts its work.
    ///
    /// ```
    /// use corten::{prelude::*, Href};
    ///
    /// #[derive(Route)]
    /// #[route("/reports/suggested", prefetch = false)]
    /// struct Suggested;
    ///
    /// #[derive(Route)]
    /// #[route("/reports")]
    /// struct Reports;
    ///
    /// let links = html! { <a href={Suggested}>"Suggested"</a><a href={Reports}>"Reports"</a> };
    /// assert_eq!(links.to_html(), r#"<a href="/reports/suggested" data-no-prefetch>Suggested</a><a href="/reports">Reports</a>"#);
    /// // As a prop too, where the link is any route.
    /// let to: &dyn Href = &Suggested;
    /// assert_eq!(html! { <a href={to}>"Suggested"</a> }.to_html(), r#"<a href="/reports/suggested" data-no-prefetch>Suggested</a>"#);
    /// // A form's action is never prefetched, so it carries nothing.
    /// assert_eq!(html! { <form action={Suggested}></form> }.to_html(), r#"<form action="/reports/suggested"></form>"#);
    /// ```
    ///
    /// In a routes module it goes after the handlers:
    ///
    /// ```
    /// use corten::{prelude::*, route::Route};
    ///
    /// #[corten::routes]
    /// mod url {
    ///     #[route("/reports/suggested", get = suggested, prefetch = false)]
    ///     pub struct Suggested;
    /// }
    ///
    /// async fn suggested() -> &'static str {
    ///     "42 seconds of pricing"
    /// }
    ///
    /// fn main() {
    ///     assert!(!url::Suggested::PREFETCH);
    ///     let _ = App::new(url::router());
    /// }
    /// ```
    ///
    /// The value is `false` or nothing:
    ///
    /// ```compile_fail
    /// #[derive(corten::Route)]
    /// #[route("/reports", prefetch = no)]
    /// struct Reports;
    /// ```
    ///
    /// and the default is not written out, so there is one way to say it:
    ///
    /// ```compile_fail
    /// #[derive(corten::Route)]
    /// #[route("/reports", prefetch = true)]
    /// struct Reports;
    /// ```
    const PREFETCH: bool = true;

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
    /// a full load and after a client navigation alike. A link to a place on the page it is in is
    /// [`Id::fragment`](crate::view::Id::fragment).
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

/// A borrowed route is the same URL, so an API that takes a route by value
/// (`ReadAgain::from(route)`) takes `&route` as well.
impl<R: Route + ?Sized> Route for &R {
    const PATH: &'static str = R::PATH;
    const WRITTEN: &'static str = R::WRITTEN;

    fn write_href(&self, out: &mut String, html: bool) {
        (**self).write_href(out, html);
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
pub(crate) fn write_fragment(id: &(impl crate::view::Id + ?Sized), out: &mut String) {
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
    fn write_link(&self, out: &mut String, html: bool) {
        self.write_href(out, html)
    }

    fn prefetch(&self) -> bool {
        R::PREFETCH
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
        no_prefetch(name, R::PREFETCH, out);
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
/// from another's copies one value. A route's handler reads them back with the route's other
/// parameters, [`Params`].
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
/// // A route's handler reads them back as the field of its `Params<Items>`, each by its key as
/// // the route reads its own, which needs no `Deserialize`. The filter form that posts the same
/// // names (`Fields`) is read by serde:
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
pub trait QueryParams {
    /// The names it writes, which a route that includes it checks against its own when it
    /// compiles. Written by the derive.
    #[doc(hidden)]
    const __KEYS: &'static [__private::Key];

    /// Writes each parameter as `?key=value` or `&key=value` (`&amp;` with `html`), `first`
    /// saying which. Implemented by `#[derive(Route)] #[route(query)]`, and for a [`FromQuery`]
    /// set by its link.
    fn write_params(&self, first: &mut bool, html: bool, out: &mut String);
}

/// A `#[route(query)]` set whose value is a rule over what was sent, for a set the derive cannot
/// read as the value itself: one that counts only when every value sent for a key agrees (a link
/// edited by hand sends one twice, which a plain field refuses), or that looks across keys
/// (`filtered` when any `dimension.*` has a value).
///
/// Two derived sets carry its names. Its [`Link`](FromQuery::Link) writes its links, as narrow as
/// the value (one listing), so a link cannot write what the value cannot hold. Its
/// [`Read`](FromQuery::Read) is what the derive reads back, every value sent (`Vec` fields), and
/// [`from_query`](FromQuery::from_query) turns it into the value, so every handler gets the rule
/// already applied. The two must have the same names, and a route that includes the set checks
/// them against its own, when the crate compiles. `from_query` must give back the value `link`
/// wrote, so a link and its handler agree: test that round trip through the app's router. A
/// derived set reads itself, so implementing this for one does not compile.
///
/// ```
/// use corten::{form::FormError, prelude::*, route::FromQuery};
///
/// /// What the buyer chose: a listing only when every `listing` sent names the same one.
/// #[derive(Debug, Default, PartialEq)]
/// pub struct Choice { pub listing: Option<String>, pub options: Vec<(String, String)> }
///
/// #[derive(Route)]
/// #[route(query)]
/// pub struct ChoiceLink<'a> {
///     pub listing: Option<&'a str>,
///     #[route(prefix = "dimension.")]
///     pub options: &'a [(String, String)],
/// }
///
/// #[derive(Route)]
/// #[route(query)]
/// pub struct ChoiceRead {
///     pub listing: Vec<String>,
///     #[route(prefix = "dimension.")]
///     pub options: Vec<(String, String)>,
/// }
///
/// impl FromQuery for Choice {
///     type Link<'a> = ChoiceLink<'a>;
///     type Read = ChoiceRead;
///
///     fn link(&self) -> ChoiceLink<'_> {
///         ChoiceLink { listing: self.listing.as_deref(), options: &self.options }
///     }
///
///     fn from_query(read: ChoiceRead) -> Result<Self, FormError> {
///         let mut listings = read.listing.into_iter().filter(|l| !l.is_empty());
///         let first = listings.next();
///         let listing = if listings.all(|l| Some(&l) == first.as_ref()) { first } else { None };
///         Ok(Choice { listing, options: read.options })
///     }
/// }
///
/// #[derive(Route)]
/// #[route("/items/{slug}")]
/// pub struct Item<'a> { pub slug: &'a str, #[route(query)] pub choice: Choice }
///
/// let choice = Choice { listing: Some("l1".into()), options: vec![("form".into(), "graded".into())] };
/// assert_eq!(Item { slug: "pika", choice }.href(), "/items/pika?listing=l1&dimension.form=graded");
///
/// // A handler of the route reads it as its `Params` field, through `Read` and `from_query`:
/// async fn item(Params(item): Params<Item<'_>>) -> String {
///     format!("{} {:?}", item.slug, item.choice.listing)
/// }
/// # let _ = item;
/// ```
///
/// A read set with a name its link does not write (or the other way) does not compile:
///
/// ```compile_fail,E0080
/// use corten::{form::FormError, prelude::*, route::FromQuery};
///
/// #[derive(Route)]
/// #[route(query)]
/// pub struct ChoiceLink<'a> { pub listing: Option<&'a str> }
///
/// #[derive(Route)]
/// #[route(query)]
/// pub struct ChoiceRead { pub listng: Vec<String> }
///
/// pub struct Choice { pub listing: Option<String> }
///
/// impl FromQuery for Choice {
///     type Link<'a> = ChoiceLink<'a>;
///     type Read = ChoiceRead;
///     fn link(&self) -> ChoiceLink<'_> { ChoiceLink { listing: self.listing.as_deref() } }
///     fn from_query(read: ChoiceRead) -> Result<Self, FormError> {
///         Ok(Choice { listing: read.listng.into_iter().next() })
///     }
/// }
///
/// #[derive(Route)]
/// #[route("/items")]
/// pub struct Items { #[route(query)] pub choice: Choice }
/// ```
///
/// Nor does one whose name is a prefix where the link's is a parameter:
///
/// ```compile_fail,E0080
/// use corten::{form::FormError, prelude::*, route::FromQuery};
///
/// #[derive(Route)]
/// #[route(query)]
/// pub struct ChoiceLink<'a> { pub listing: Option<&'a str> }
///
/// #[derive(Route)]
/// #[route(query)]
/// pub struct ChoiceRead { #[route(prefix = "listing")] pub listing: Vec<(String, String)> }
///
/// pub struct Choice { pub listing: Option<String> }
///
/// impl FromQuery for Choice {
///     type Link<'a> = ChoiceLink<'a>;
///     type Read = ChoiceRead;
///     fn link(&self) -> ChoiceLink<'_> { ChoiceLink { listing: self.listing.as_deref() } }
///     fn from_query(read: ChoiceRead) -> Result<Self, FormError> {
///         Ok(Choice { listing: read.listing.into_iter().next().map(|(_, v)| v) })
///     }
/// }
///
/// #[derive(Route)]
/// #[route("/items")]
/// pub struct Items { #[route(query)] pub choice: Choice }
/// ```
///
/// A name the read set reads and the link never writes, an older name of a parameter that links
/// now write another way (so an address shared before the rename still reads), is
/// `#[route(read_only)]` on the read set's field. No link writes it, and the route counts it with
/// the others, so no other parameter can have it:
///
/// ```
/// use corten::{form::FormError, prelude::*, route::FromQuery};
///
/// #[derive(Route)]
/// #[route(query)]
/// pub struct TermsLink<'a> { pub q: Option<&'a str> }
///
/// #[derive(Route)]
/// #[route(query)]
/// pub struct TermsRead { pub q: Vec<String>, #[route(read_only)] pub search: Vec<String> }
///
/// pub struct Terms(pub Option<String>);
///
/// impl FromQuery for Terms {
///     type Link<'a> = TermsLink<'a>;
///     type Read = TermsRead;
///     fn link(&self) -> TermsLink<'_> { TermsLink { q: self.0.as_deref() } }
///     fn from_query(read: TermsRead) -> Result<Self, FormError> {
///         // `q`, or the older `search` when there is no `q`.
///         Ok(Terms(read.q.into_iter().chain(read.search).find(|t| !t.is_empty())))
///     }
/// }
///
/// #[derive(Route)]
/// #[route("/search")]
/// pub struct Search { #[route(query)] pub terms: Terms }
///
/// assert_eq!(Search { terms: Terms(Some("pika".into())) }.href(), "/search?q=pika");
/// ```
///
/// ```compile_fail,E0080
/// use corten::{form::FormError, prelude::*, route::FromQuery};
///
/// #[derive(Route)]
/// #[route(query)]
/// pub struct TermsLink<'a> { pub q: Option<&'a str> }
///
/// #[derive(Route)]
/// #[route(query)]
/// pub struct TermsRead { pub q: Vec<String>, #[route(read_only)] pub search: Vec<String> }
///
/// pub struct Terms(pub Option<String>);
///
/// impl FromQuery for Terms {
///     type Link<'a> = TermsLink<'a>;
///     type Read = TermsRead;
///     fn link(&self) -> TermsLink<'_> { TermsLink { q: self.0.as_deref() } }
///     fn from_query(read: TermsRead) -> Result<Self, FormError> { Ok(Terms(read.q.into_iter().next())) }
/// }
///
/// // `search` is the set's older name, so the route cannot have a parameter of its own by it.
/// #[derive(Route)]
/// #[route("/search")]
/// pub struct Search<'a> { pub search: Option<&'a str>, #[route(query)] pub terms: Terms }
/// ```
///
/// A read-only name is one query parameter's: not a path parameter, a set or a prefix.
///
/// ```compile_fail
/// use corten::prelude::*;
///
/// #[derive(Route)]
/// #[route("/items/{id}")]
/// pub struct Item { #[route(read_only)] pub id: String }
/// ```
///
/// The route checks its own names against the set's, so a route that writes `listing` itself
/// beside it does not compile:
///
/// ```compile_fail,E0080
/// use corten::{form::FormError, prelude::*, route::FromQuery};
///
/// #[derive(Route)]
/// #[route(query)]
/// pub struct ChoiceLink<'a> { pub listing: Option<&'a str> }
///
/// #[derive(Route)]
/// #[route(query)]
/// pub struct ChoiceRead { pub listing: Vec<String> }
///
/// pub struct Choice { pub listing: Option<String> }
///
/// impl FromQuery for Choice {
///     type Link<'a> = ChoiceLink<'a>;
///     type Read = ChoiceRead;
///     fn link(&self) -> ChoiceLink<'_> { ChoiceLink { listing: self.listing.as_deref() } }
///     fn from_query(read: ChoiceRead) -> Result<Self, FormError> {
///         Ok(Choice { listing: read.listing.into_iter().next() })
///     }
/// }
///
/// #[derive(Route)]
/// #[route("/items")]
/// pub struct Items { pub listing: Option<String>, #[route(query)] pub choice: Choice }
/// ```
///
/// A derived set, which writes and reads itself, cannot implement it too (a borrowed one
/// included):
///
/// ```compile_fail,E0119
/// use corten::{form::FormError, prelude::*, route::FromQuery};
///
/// #[derive(Route)]
/// #[route(query)]
/// pub struct Paging<'a> { pub page: Option<&'a str> }
///
/// #[derive(Route)]
/// #[route(query)]
/// pub struct PagingRead { pub page: Vec<String> }
///
/// impl FromQuery for Paging<'static> {
///     type Link<'a> = Paging<'a>;
///     type Read = PagingRead;
///     fn link(&self) -> Paging<'_> { Paging { page: self.page } }
///     fn from_query(_: PagingRead) -> Result<Self, FormError> { Ok(Paging { page: None }) }
/// }
/// ```
pub trait FromQuery: Sized + 'static {
    /// The derived `#[route(query)]` set that writes its links.
    type Link<'a>: QueryParams
    where
        Self: 'a;

    /// The derived, owned `#[route(query)]` set read back from the request, with the link's
    /// names: every value sent, for [`from_query`](FromQuery::from_query) to judge.
    type Read: QueryParams;

    /// The link's set for this value.
    fn link(&self) -> Self::Link<'_>;

    /// The value, from what the request sent. A failure refuses the page with a 400, as a
    /// parameter that does not read does; make one with `serde::de::Error::custom`, its message
    /// starting with the parameter's name as a field's does (`"listing: two different listings"`).
    fn from_query(read: Self::Read) -> Result<Self, crate::form::FormError>;
}

impl<T: FromQuery> QueryParams for T {
    const __KEYS: &'static [__private::Key] = {
        __private::same_keys(<T::Link<'static> as QueryParams>::__KEYS, <T::Read as QueryParams>::__KEYS);
        // The read set's: the names links write and any it only reads, which no other may take.
        <T::Read as QueryParams>::__KEYS
    };

    #[inline]
    fn write_params(&self, first: &mut bool, html: bool, out: &mut String) {
        self.link().write_params(first, html, out)
    }
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
///
/// // Its handler reads them back with the other parameters, the prefix taken off:
/// async fn item(Params(item): Params<Item<'_>>) -> String {
///     format!("{} {:?}", item.slug, item.dimensions) // dimensions: BTreeMap<String, String>
/// }
/// # let _ = item;
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
/// ([`Route::at`]), a return address checked to be on this site ([`Local`]), or an
/// `axum::http::Uri` for a URL the app does not route (a payment page, a sign-in provider). Text is none of them, so a path typed as a string does not compile. Used by
/// [`crate::Req::redirect`] and [`crate::Req::is_current`].
///
/// It is also a link a view writes, `href={..}`: a component several pages draw takes where its
/// links and forms go as `&dyn Href`, so each page passes its own typed route, and an enum
/// whose variants each hold a route (an app's screens) derives it:
///
/// ```
/// use corten::{prelude::*, Href};
///
/// #[derive(Route)] #[route("/inventory")] struct Inventory;
/// #[derive(Route)] #[route("/pull-sheet/{id}")] struct PullSheet<'a> { id: &'a str, sort: Option<&'a str> }
///
/// #[derive(Href)]
/// enum Screen<'a> { Inventory(Inventory), PullSheet(PullSheet<'a>), Given(&'a dyn Href) }
///
/// #[component]
/// fn Filter<'a>(action: &'a dyn Href) -> impl Render {
///     html! { <form method="get" action={action}><input name="q"></form> }
/// }
///
/// let to = Screen::PullSheet(PullSheet { id: "7", sort: Some("a&b") });
/// assert_eq!(html! { <a href={to}>"Pull sheet"</a> }.to_html(), r#"<a href="/pull-sheet/7?sort=a%26b">Pull sheet</a>"#);
/// let here = PullSheet { id: "7", sort: None };
/// assert_eq!(Filter { action: &here }.to_html(), r#"<form method="get" action="/pull-sheet/7"><input name="q"></form>"#);
/// assert_eq!(Screen::Inventory(Inventory).with_href(str::to_owned), "/inventory");
/// // A prop a component was given goes in one too, and `link` reads any of them.
/// assert_eq!(corten::route::link(&Screen::Given(&here)), "/pull-sheet/7");
/// ```
///
/// Each variant holds one link; a unit variant has no URL:
///
/// ```compile_fail
/// use corten::{prelude::*, Href};
///
/// #[derive(Route)] #[route("/inventory")] struct Inventory;
///
/// #[derive(Href)]
/// enum Screen { Inventory(Inventory), Settings }
/// # fn main() {}
/// ```
///
/// Nor is a variant that holds a link and more (the data belongs in the route):
///
/// ```compile_fail
/// use corten::{prelude::*, Href};
///
/// #[derive(Route)] #[route("/inventory")] struct Inventory;
///
/// #[derive(Href)]
/// enum Screen { Inventory(Inventory, u32) }
/// # fn main() {}
/// ```
///
/// Where one enum of every page's route would join code that should stay apart (a shared
/// component whose pages live in separate feature modules), its prop is a function that returns a
/// boxed link, and `href={..}` takes the box as it is:
///
/// ```
/// use corten::{prelude::*, Href};
///
/// #[derive(Route)] #[route("/pull-sheet")] struct PullSheet<'a> { sort: Option<&'a str> }
///
/// #[component]
/// fn SortLinks<'a>(link: &'a dyn Fn(&'static str) -> Box<dyn Href + 'a>) -> impl Render {
///     html! { <a href={link("price")}>"Price"</a> <a href={&link("name")}>"Name"</a> }
/// }
///
/// let link = |sort| Box::new(PullSheet { sort: Some(sort) }) as Box<dyn Href>;
/// assert_eq!(
///     SortLinks { link: &link }.to_html(),
///     r#"<a href="/pull-sheet?sort=price">Price</a><a href="/pull-sheet?sort=name">Name</a>"#
/// );
/// ```
///
/// A value that is neither text nor a link is an error about the attribute:
///
/// ```compile_fail,E0277
/// use corten::prelude::*;
///
/// struct Sort;
/// let _ = html! { <a href={Sort}>"Price"</a> };
/// ```
///
/// Text is not a link, so a `&str` is not a `&dyn Href`:
///
/// ```compile_fail,E0277
/// use corten::{prelude::*, Href};
///
/// #[component]
/// fn Filter<'a>(action: &'a dyn Href) -> impl Render {
///     html! { <form action={action}></form> }
/// }
///
/// let _ = Filter { action: &"/pull-sheet/7" };
/// ```
pub trait Href {
    /// Writes the URL; with `html`, escaped for an attribute's value (`&` as `&amp;`).
    fn write_link(&self, out: &mut String, html: bool);

    /// Calls `f` with the URL.
    fn with_href<T>(&self, f: impl FnOnce(&str) -> T) -> T
    where
        Self: Sized,
    {
        match self.as_text() {
            Some(text) => f(text),
            None => f(&link(self)),
        }
    }

    /// The URL, when the link holds it as text already (a [`Local`]), so a redirect reads it
    /// without writing it again.
    #[doc(hidden)]
    fn as_text(&self) -> Option<&str> {
        None
    }

    /// Whether the runtime may fetch the page ahead of a click: a route's [`Route::PREFETCH`].
    fn prefetch(&self) -> bool {
        true
    }
}

/// After `href="..."`, the opt-out a link to a page not to prefetch carries. Only for `href`: a
/// form's `action` is never prefetched.
#[doc(hidden)]
#[inline]
pub fn no_prefetch(name: &str, prefetch: bool, out: &mut String) {
    if !prefetch && name == " href" {
        out.push_str(" data-no-prefetch");
    }
}

/// The URL of any [`Href`] as a header value: `Location` for a redirect.
pub(crate) fn header_value(to: &(impl Href + ?Sized)) -> Result<axum::http::HeaderValue, axum::http::header::InvalidHeaderValue> {
    // From text, not from the `String`: one with room to spare would cost `Bytes` an allocation of
    // its own.
    match to.as_text() {
        Some(text) => axum::http::HeaderValue::from_str(text),
        None => axum::http::HeaderValue::from_str(&link(to)),
    }
}

pub use corten_macros::Href;

/// The URL of any [`Href`], a `dyn Href` included, not escaped: `link(to)` where a `Sized` value
/// would call [`Href::with_href`].
pub fn link(to: &(impl Href + ?Sized)) -> String {
    let mut out = String::with_capacity(32);
    to.write_link(&mut out, false);
    out
}

macro_rules! dyn_href {
    ($($ty:ty),*) => {$(
        impl crate::view::AttrValue for $ty {
            fn write_attr(&self, name: &str, out: &mut String) {
                out.push_str(name);
                out.push_str("=\"");
                self.write_link(out, true);
                out.push('"');
                no_prefetch(name, self.prefetch(), out);
            }
        }

        /// A link held by reference (a component's prop), so an enum of links can hold one.
        impl Href for &$ty {
            fn write_link(&self, out: &mut String, html: bool) {
                (**self).write_link(out, html)
            }

            fn as_text(&self) -> Option<&str> {
                (**self).as_text()
            }

            fn prefetch(&self) -> bool {
                (**self).prefetch()
            }
        }

        impl Href for Box<$ty> {
            fn write_link(&self, out: &mut String, html: bool) {
                (**self).write_link(out, html)
            }

            fn as_text(&self) -> Option<&str> {
                (**self).as_text()
            }

            fn prefetch(&self) -> bool {
                (**self).prefetch()
            }
        }

        /// A boxed link (one a shared component's function prop returns) is written as the
        /// link it holds: `href={(paging.link)(by)}`.
        impl crate::view::AttrValue for Box<$ty> {
            fn write_attr(&self, name: &str, out: &mut String) {
                (**self).write_attr(name, out)
            }
        }
    )*};
}
dyn_href!(dyn Href + '_, dyn Href + Send + '_, dyn Href + Send + Sync + '_);

/// A path on this site taken from outside the code, a return address above all
/// (`/sign-in?next=/account/cart`): what [`Local::parse`] accepts can only lead to this site, so
/// `req.redirect(&next)` cannot send the user elsewhere (an open redirect). Whether the app has a
/// route a redirect can land on there is [`crate::Req::serves`]. `to_string()` gives the whole
/// address, query and fragment included.
///
/// ```
/// use corten::Local;
///
/// assert_eq!(Local::parse("/account/cart?step=2#pay").unwrap().path(), "/account/cart");
/// // Each of these leads a browser to another site, or is not a path: none is a Local.
/// for other in ["//evil.example", "/\\evil.example", "/\t/evil.example", "https://evil.example", "account", ""] {
///     assert!(Local::parse(other).is_none(), "{other:?}");
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Local(String);

impl Local {
    /// `text` when it is a path on this site: it starts with one `/`, not `//` or `/\` (which a
    /// browser reads as another host), and is printable ASCII without `\`, so no control
    /// character or space a browser drops can turn it into one. Two more are refused because the
    /// browser would ask for another path than the one [`crate::Req::serves`] checks: a `.` or
    /// `..` segment, which it resolves away, and in the path a percent-escaped letter, digit or
    /// `-._~` (`%61`, `%2e`), which Chromium sends as the plain character. Anything else, a full
    /// URL included, is `None`: send the user to the fallback.
    pub fn parse(text: &str) -> Option<Local> {
        let bytes = text.as_bytes();
        let path = bytes.first() == Some(&b'/') && !matches!(bytes.get(1), Some(b'/' | b'\\'));
        let printable = bytes.iter().all(|&b| b.is_ascii_graphic() && b != b'\\');
        let route = text.split(['?', '#']).next().unwrap_or("");
        let dots = route.split('/').any(|segment| segment == "." || segment == "..");
        let unreserved = |pair: &[u8]| {
            let hex = |b: u8| (b as char).to_digit(16);
            match (hex(pair[0]), hex(pair[1])) {
                (Some(high), Some(low)) => {
                    let b = (high * 16 + low) as u8;
                    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~')
                }
                _ => false,
            }
        };
        let escaped = route.as_bytes().split(|&b| b == b'%').skip(1).any(|after| after.len() >= 2 && unreserved(&after[..2]));
        (path && printable && !dots && !escaped).then(|| Local(text.to_string()))
    }

    /// The path, without the query or fragment: what the app's routes are matched against.
    pub fn path(&self) -> &str {
        self.0.split(['?', '#']).next().unwrap_or("")
    }
}

impl std::fmt::Display for Local {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl Href for Local {
    fn write_link(&self, out: &mut String, html: bool) {
        write_text(&self.0, out, html)
    }

    fn as_text(&self) -> Option<&str> {
        Some(&self.0)
    }
}

impl Local {
    /// The whole address, as text.
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// A URL that is already text, escaped for an attribute with `html`.
fn write_text(url: &str, out: &mut String, html: bool) {
    if html {
        crate::view::escape_into(url, out)
    } else {
        out.push_str(url)
    }
}

/// A byte a route's path has as text, which a browser sends as it is.
const fn plain(c: u8) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(c, b'-' | b'.' | b'_' | b'~' | b'!' | b'$' | b'*' | b'+' | b',' | b';' | b'=' | b':' | b'@')
}

/// Whether `path` has the shape `#[derive(Route)]` writes, checked when the app compiles so a
/// `Route` written by hand is held to it too: each segment is `{name}` (lowercase letters, digits
/// and `_`), `{*name}` as the last, or text a browser sends as it is (letters, digits and
/// `-._~!$*+,;=:@`); [`with_suffixes`] adds a route with text after a parameter (`{name}.xml`), and
/// [`chosen`] matches both shapes, as the router does. (The derive also
/// refuses a name used twice; that changes no route's match, so it is not checked here.)
pub(crate) const fn route_shaped(path: &str) -> bool {
    const fn name(c: u8, first: bool) -> bool {
        c.is_ascii_lowercase() || c == b'_' || (!first && c.is_ascii_digit())
    }
    let b = path.as_bytes();
    if b.is_empty() || b[0] != b'/' {
        return false;
    }
    let mut start = 1;
    while start <= b.len() {
        let mut end = start;
        while end < b.len() && b[end] != b'/' {
            end += 1;
        }
        if end > start && b[start] == b'{' {
            let mut j = start + 1;
            if j < end && b[j] == b'*' {
                if end != b.len() {
                    return false;
                }
                j += 1;
            }
            if b[end - 1] != b'}' || j >= end - 1 || !name(b[j], true) {
                return false;
            }
            while j < end - 1 {
                if !name(b[j], false) {
                    return false;
                }
                j += 1;
            }
        } else {
            let mut j = start;
            while j < end {
                if !plain(b[j]) {
                    return false;
                }
                j += 1;
            }
        }
        start = end + 1;
    }
    true
}

#[cfg(test)]
#[test]
fn a_route_path_has_the_shape_the_derive_writes() {
    for ok in ["/", "/items", "/items/{id}", "/items/{id}/edit", "/files/{*rest}", "/a//b", "/a.b~c", "/{a_1}"] {
        assert!(route_shaped(ok), "{ok}");
    }
    for bad in [
        "",
        "items",
        "/item_{id}",
        "/{id}x",
        "/a}",
        "/{}",
        "/{*}",
        "/{a/b}",
        "/{*x}/y",
        "/{Id}",
        "/{1a}",
        "/a|b",
        "/a%20b",
        "/caf\u{e9}",
        "/{{x}}",
        "/{",
        "/{*",
        "/{a",
        "/{*x}/",
    ] {
        assert!(!route_shaped(bad), "{bad}");
    }
}

/// Of `routes` (each pattern, and whether it has a `GET` handler), the one the router picks for
/// `path`: of those that match, the one whose segments are most specific from the left, text
/// before `{name}` before `{*rest}`, as matchit tries them.
pub(crate) fn chosen(routes: &[(&'static str, bool)], path: &str) -> Option<(&'static str, bool)> {
    // Each segment's rank, compared from the left without collecting them.
    fn preference(pattern: &str) -> impl Iterator<Item = u8> + '_ {
        pattern.split('/').map(|s| {
            if s.starts_with("{*") {
                0
            } else if s.starts_with('{') {
                1
            } else {
                2
            }
        })
    }
    // The router picks by each segment's shape (it matches `{page}.xml` as `{page}`); a route with
    // text after a parameter then answers 404 when the segment lacks it, so the path is not served.
    routes
        .iter()
        .copied()
        .filter(|(pattern, _)| segments_match(pattern, path, false))
        .max_by(|(a, _), (b, _)| preference(a).cmp(preference(b)))
        .filter(|(pattern, _)| routes_path(pattern, path))
}

/// Whether `path` is one `pattern` routes, as axum (matchit) matches it: `{name}` is one segment,
/// which may be empty when another follows (`/items//edit` is `/items/{id}/edit`) but not when it
/// is the last; `{*rest}` is the rest of the path (not empty); `{name}text` is a segment that
/// ends in the text with something before it (Corten checks the text, as axum matches `{name}`);
/// and the text is compared as it is written, so `/items/` is not `/items`. These are all the
/// shapes a route macro takes.
pub(crate) fn routes_path(pattern: &str, path: &str) -> bool {
    segments_match(pattern, path, true)
}

/// [`routes_path`], with the text after a parameter required (`text`) or not (as axum matches).
fn segments_match(pattern: &str, path: &str, text: bool) -> bool {
    let (mut pattern, mut path) = (pattern.split('/'), path.split('/'));
    loop {
        match (pattern.next(), path.next()) {
            (None, None) => return true,
            (Some(want), Some(got)) if want.starts_with("{*") => {
                return !got.is_empty() || path.next().is_some();
            }
            (Some(want), Some(got)) if want.starts_with('{') => {
                let after = if text { want.split_once('}').map_or("", |(_, after)| after) } else { "" };
                if after.is_empty() && got.is_empty() && pattern.clone().next().is_none() {
                    return false;
                }
                if !after.is_empty() {
                    // The text must end the segment as written, not as the tail of an escape
                    // (`a%2e` does not end in `e`: it is `a.`).
                    let Some(head) = got.strip_suffix(after).filter(|head| !head.is_empty()) else {
                        return false;
                    };
                    let head = head.as_bytes();
                    if head.ends_with(b"%") || (head.len() >= 2 && head[head.len() - 2] == b'%') {
                        return false;
                    }
                }
            }
            (Some(want), Some(got)) if want == got => {}
            _ => return false,
        }
    }
}

#[cfg(test)]
mod local_tests {
    use super::{chosen, routes_path, Local};

    #[test]
    fn a_path_matches_a_pattern_as_axum_matches_it() {
        for (pattern, path, want) in [
            ("/", "/", true),
            ("/", "/a", false),
            ("/items", "/items", true),
            ("/items", "/items/", false),
            ("/items/{id}", "/items/7", true),
            ("/items/{id}", "/items/", false),
            ("/items/{id}/edit", "/items//edit", true),
            ("/a/{x}/{y}", "/a//", false),
            ("/a/{x}/{y}", "/a//2", true),
            ("/items/{id}", "/items/7/edit", false),
            ("/items/{id}/edit", "/items/7/edit", true),
            ("/files/{*rest}", "/files/a/b", true),
            ("/files/{*rest}", "/files/a/", true),
            ("/files/{*rest}", "/files/", false),
            ("/files/{*rest}", "/files", false),
            ("/a/{x}/{y}", "/a/1/2", true),
            ("/{*rest}", "/", false),
            ("/{*rest}", "/a", true),
            ("/items/{id}", "/items/%2F", true),
            ("/check", "/%63heck", false),
            ("/sitemap/{page}.xml", "/sitemap/3.xml", true),
            ("/sitemap/{page}.xml", "/sitemap/3", false),
            ("/sitemap/{page}.xml", "/sitemap/.xml", false),
            ("/sitemap/{page}.xml", "/sitemap/3.xml.bak", false),
            ("/x/{n}e", "/x/a%2e", false),
            ("/x/{n}ab", "/x/a%ab", false),
            ("/x/{n}e", "/x/a%25e", true),
            ("/sitemap/{page}.xml/x", "/sitemap/3.xml/x", true),
        ] {
            assert_eq!(routes_path(pattern, path), want, "{pattern} {path}");
        }
        for (path, written, want) in [
            ("/items/{id}", "/items/{id}", true),
            ("/sitemap/{kind}/{page}", "/sitemap/{kind}/{page}.xml", true),
            ("/s/{a}/{b}", "/s/{a}.x/{b}.y", true),
            ("/s/{a}", "/s/{a}.x/y", false),
            ("/s/{a}", "/s/{b}.x", false),
            ("/s/x", "/s/x.y", false),
            ("/f/{*rest}", "/f/{*rest}.xml", false),
            ("/s/{a}", "/s/{a}.{b}", false),
            ("/s/{a}/b", "/s/{a}/b", true),
            // Only the text a browser sends as it is.
            ("/s/{a}", "/s/{a}.x|y", false),
            ("/s/{a}/b", "/s/{a}?x/b", false),
            ("/s/{a}", "/s/{a}%20", false),
        ] {
            assert_eq!(super::written_fits(path, written), want, "{path} {written}");
        }
        for (a, b, want) in [
            ("/feeds/{name}.rss", "/feeds/{id}.atom", true),
            ("/feeds/{name}.rss", "/feeds/{name}.rss", false),
            ("/feeds/{name}.rss", "/feeds/{id}.rss", false),
            ("/feeds/{name}.rss", "/feeds/{name}", true),
            ("/feeds/{name}.rss", "/feeds/{*rest}", false),
            ("/feeds/{name}.rss", "/other/{name}.atom", false),
            ("/a/{x}.r/b", "/a/{y}.s/b", true),
            ("/a/{x}.r/b", "/a/{y}.s/c", false),
            ("/a/{x}", "/a/{y}", false),
        ] {
            assert_eq!(super::clash(a, b), want, "{a} {b}");
            assert_eq!(super::clash(b, a), want, "{b} {a}");
        }
        for (path, other, beside) in [
            ("/{name}.txt", "/{*page}", true),
            ("/docs/{name}.md", "/docs/{*rest}", true),
            ("/{kind}.x/{name}", "/{a}/{*rest}", true),
            ("/docs/{name}.md", "/{*page}", false),
            ("/docs/{name}.md", "/blog/{*rest}", false),
            ("/{*page}", "/{*page}", false),
        ] {
            assert_eq!(super::beside_catch_all(path, other), beside, "{path} beside {other}");
        }
    }

    #[test]
    fn the_route_the_router_picks_decides() {
        let routes = [
            ("/cart/{id}", true),
            ("/cart/add", false),
            ("/{*page}", true),
            ("/items/{id}", true),
            ("/items/{id}/{*rest}", false),
        ];
        for (path, want) in [
            ("/cart/7", Some(("/cart/{id}", true))),
            ("/cart/add", Some(("/cart/add", false))),
            ("/about/team", Some(("/{*page}", true))),
            ("/items/7", Some(("/items/{id}", true))),
            ("/items/7/x", Some(("/items/{id}/{*rest}", false))),
            ("/", None),
        ] {
            assert_eq!(chosen(&routes, path), want, "{path}");
        }
    }

    #[test]
    fn only_a_path_on_this_site_is_local() {
        for ok in [
            "/",
            "/a",
            "/a?b=c&d=%2F%2F",
            "/a#x",
            "/%2F%2Fevil.example",
            "/a//b",
            "/a/.b",
            "/a/b..",
            "/a?x=/../",
            "/a?q=%61",
            "/a%20b",
            "/a%2fb",
            "/50%",
            "/a%2",
        ] {
            assert_eq!(Local::parse(ok).map(|l| l.to_string()).as_deref(), Some(ok), "{ok:?}");
        }
        for bad in [
            "",
            "a",
            "//evil",
            "/\\evil",
            "/a\\b",
            "/\t/evil",
            "/\n/evil",
            "/a b",
            "/caf\u{e9}",
            "/\x7f",
            "http://evil",
            " /a",
            "/a/./b",
            "/a/..",
            "/./a",
            "/a/%2e%2E/b",
            "/a/.%2E",
            "/items/..",
            "/cart/%61dd",
            "/%7Euser",
            "/a%2Db",
            "/a%5Fb",
            "/%31",
        ] {
            assert!(Local::parse(bad).is_none(), "{bad:?}");
        }
        assert_eq!(Local::parse("/a/b?c#d").unwrap().path(), "/a/b");
        assert_eq!(Local::parse("/a#d?x").unwrap().path(), "/a");
    }

    /// A link written as `dyn Href` is escaped for the attribute, whatever it holds.
    #[test]
    fn a_dyn_href_is_escaped_for_its_attribute() {
        use super::{link, Href};
        use crate::view::AttrValue;
        let local = Local::parse("/a?q=\"x\"&b='<y>'").unwrap();
        let uri = axum::http::Uri::from_static("https://example.com/p?a=1&b=2");
        for (to, want) in [
            (&local as &dyn Href, r#" href="/a?q=&quot;x&quot;&amp;b=&#39;&lt;y&gt;&#39;""#),
            (&uri as &dyn Href, r#" href="https://example.com/p?a=1&amp;b=2""#),
        ] {
            let mut out = String::new();
            to.write_attr(" href", &mut out);
            assert_eq!(out, want);
            assert!(!link(to).contains("&amp;"), "plain for a redirect");
        }
    }
}

impl Href for axum::http::Uri {
    fn write_link(&self, out: &mut String, html: bool) {
        write_text(&self.to_string(), out, html)
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
    fn write_link(&self, out: &mut String, html: bool) {
        write_text(&self.href(), out, html)
    }
}

impl<R: Route + ?Sized> Href for R {
    fn write_link(&self, out: &mut String, html: bool) {
        self.write_href(out, html)
    }

    fn prefetch(&self) -> bool {
        R::PREFETCH
    }
}

/// A link to a page of this app: a typed route, a place on one (`route.at(id)`), a [`Local`]
/// return address, or a `#[derive(Href)]` enum whose variants all are. Taken where only a page of
/// the app's own works: [`crate::Req::redirect_with_notice`] (a notice is shown by the app's page),
/// and a web app manifest's `id` and shortcuts ([`crate::pwa::Manifest::id`],
/// [`crate::pwa::Shortcut::new`]), which a browser drops when they are on another origin. An
/// `axum::http::Uri`, a string or an [`Elsewhere`] is not one. Sealed: Corten and the derive
/// implement it.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a page of this app",
    label = "not a typed route, `route.at(id)`, `Local` or `#[derive(Href)]` enum of those",
    note = "only a page of this app fits here: name it by its route (to send a request to another site, use `req.redirect`)"
)]
pub trait OnSite: Href + on_site::Sealed {}

#[doc(hidden)]
pub mod on_site {
    /// Keeps [`super::OnSite`] to the links Corten knows are the app's; `#[derive(Href)]`
    /// implements it for an enum whose variants all are.
    pub trait Sealed {}
}

impl<R: Route + ?Sized> on_site::Sealed for R {}
impl<R: Route + ?Sized> OnSite for R {}

impl<R: Route + ?Sized, I: crate::view::Id> on_site::Sealed for At<'_, R, I> {}
impl<R: Route + ?Sized, I: crate::view::Id> OnSite for At<'_, R, I> {}

impl on_site::Sealed for Local {}
impl OnSite for Local {}

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

/// A route's parameters, read from the request: what a handler of a route with parameters
/// takes, `Params<url::Item<'_>>`. It has the route's fields, owned (a `&'a str` or `Cow<'a, str>`
/// is a `String`, a `&'a [T]` a `Vec<T>`, any other `&'a T` a `T`), filled the way a link writes them:
/// a path parameter from its segment, percent-decoded; a query parameter by the name serde reads
/// (`None`, or empty for a list, when the query leaves it out; a field with `#[serde(default)]`
/// its default); a `#[route(query)]` field's set each of its fields the same way; a
/// `#[route(prefix = "dim.")]` field from each parameter with the prefix, taken off. A word an
/// enum does not have (an old bookmark) reads as missing, and a prefixed entry holding one is
/// left out; a request whose parameters otherwise do not fit is a `400 Bad Request`.
///
/// So the link and the handler cannot disagree about a parameter's name or type, and a handler
/// of a route with a parameter in its path must take the route's `Params`: in a
/// `#[corten::routes]` module, one that reads another route's, or none, does not compile.
///
/// ```
/// use corten::prelude::*;
///
/// #[corten::routes]
/// mod url {
///     #[route("/items/{id}", get = show)]
///     pub struct Item<'a> {
///         pub id: &'a str,
///         pub tab: Option<&'a str>,
///     }
/// }
///
/// async fn show(item: Params<url::Item<'_>>) -> String {
///     format!("{} on {}", item.id, item.tab.as_deref().unwrap_or("details"))
/// }
///
/// let app = App::new(url::router());
/// # let _ = app;
/// assert_eq!(url::Item { id: "a/b", tab: Some("offers") }.href(), "/items/a%2Fb?tab=offers");
/// ```
///
/// A route names its handler from where the module is declared, so a `use` inside the module
/// that brings one in does not compile, and the error is at the `use`:
///
/// ```compile_fail
/// mod page {
///     pub async fn show() -> &'static str {
///         "hi"
///     }
/// }
///
/// #[corten::routes]
/// mod url {
///     use super::page::{show as home};
///
///     #[route("/", get = home)]
///     pub struct Home;
/// }
/// # fn main() {}
/// ```
///
/// and neither does a handler written inside the module:
///
/// ```compile_fail
/// #[corten::routes]
/// mod url {
///     async fn home() -> &'static str {
///         "hi"
///     }
///
///     #[route("/", get = home)]
///     pub struct Home;
/// }
/// # fn main() {}
/// ```
///
/// A `use` the module needs for itself (a field's type, here a module a handler is also named
/// through) is left alone:
///
/// ```
/// mod page {
///     pub async fn show() -> &'static str {
///         "hi"
///     }
///     pub type Tab = u8;
/// }
/// use page::show as home;
///
/// #[corten::routes]
/// mod url {
///     use super::page;
///
///     #[route("/", get = home)]
///     pub struct Home;
///
///     #[route("/page", get = page::show)]
///     pub struct Page {
///         pub tab: Option<page::Tab>,
///     }
/// }
/// # fn main() {
/// #     let _ = url::router();
/// # }
/// ```
///
/// A handler that reads its parameters some other way, or another route's, does not compile:
///
/// ```compile_fail,E0277
/// use corten::{axum::extract::Path, prelude::*};
///
/// #[corten::routes]
/// mod url {
///     #[route("/items/{id}", get = show)]
///     pub struct Item<'a> { pub id: &'a str }
/// }
///
/// async fn show(Path(id): Path<String>) -> String {
///     id
/// }
/// # fn main() {}
/// ```
///
/// ```compile_fail,E0277
/// use corten::prelude::*;
///
/// #[corten::routes]
/// mod url {
///     #[route("/items/{id}", get = show)]
///     pub struct Item<'a> { pub id: &'a str }
///
///     #[route("/sellers/{seller}", get = seller)]
///     pub struct Seller<'a> { pub seller: &'a str }
/// }
///
/// async fn show(item: Params<url::Seller<'_>>) -> String {
///     item.seller.clone()
/// }
///
/// async fn seller(seller: Params<url::Seller<'_>>) -> String {
///     seller.seller.clone()
/// }
/// # fn main() {}
/// ```
///
/// `any = relay` serves every method (a relay to another server behind a catch-all), so it is the
/// route's only handler:
///
/// ```compile_fail
/// use corten::{axum::extract::Request, prelude::*, route::FromUrl};
///
/// #[corten::routes]
/// mod url {
///     #[route("/{*rest}", get = relay, any = relay)]
///     pub struct Elsewhere<'a> { pub rest: &'a str }
/// }
///
/// async fn relay<R: FromUrl>(_: Params<R>, _: Request) -> &'static str {
///     "upstream"
/// }
/// # fn main() {}
/// ```
///
/// A generic handler is named without type arguments: each route picks it by the `Params` it
/// takes, so `any = relay::<..>` does not compile:
///
/// ```compile_fail
/// use corten::{axum::extract::Request, prelude::*, route::FromUrl};
///
/// #[corten::routes]
/// mod url {
///     #[route("/{*rest}", any = relay::<Elsewhere<'static>>)]
///     pub struct Elsewhere<'a> { pub rest: &'a str }
/// }
///
/// async fn relay<R: FromUrl>(_: Params<R>, _: Request) -> &'static str {
///     "upstream"
/// }
/// # fn main() {}
/// ```
///
/// A field `Params` has no owned form for (a borrowed `#[route(query)]` set) or a `#[serde(..)]`
/// key it does not follow (`alias`, `with`, `flatten`, `skip`, a container `default`, ...),
/// on the route or on a query set it includes, leaves the route's links as they are, and a handler that
/// takes its `Params` does not compile, rather than read the parameters differently from how
/// links write them:
///
/// ```compile_fail,E0277
/// use corten::prelude::*;
///
/// #[derive(Route)]
/// #[route(query)]
/// pub struct Filters<'a> { pub q: Option<&'a str> }
///
/// #[corten::routes]
/// mod url {
///     #[route("/items/{id}", get = show)]
///     pub struct Item<'a> { pub id: &'a str, #[route(query)] pub filters: super::Filters<'a> }
/// }
///
/// async fn show(item: Params<url::Item<'_>>) -> String {
///     item.id.clone()
/// }
/// # fn main() {}
/// ```
///
/// ```compile_fail,E0277
/// use corten::prelude::*;
///
/// #[derive(Default, Route)]
/// #[route(query)]
/// pub struct Filters { #[serde(alias = "query")] pub q: Option<String> }
///
/// #[corten::routes]
/// mod url {
///     #[route("/items/{id}", get = show)]
///     pub struct Item<'a> { pub id: &'a str, #[route(query)] pub filters: super::Filters }
/// }
///
/// async fn show(item: Params<url::Item<'_>>) -> String {
///     item.id.clone()
/// }
/// # fn main() {}
/// ```
///
/// A set's container `#[serde(default)]` is refused the same way: a link of a set whose default
/// has `sort: Some("name")` writes no `sort` for `None`, which the default would read back as
/// `Some("name")`. A field's own `#[serde(default)]` is followed.
///
/// ```compile_fail,E0277
/// use corten::prelude::*;
///
/// #[derive(Default, Route)]
/// #[route(query)]
/// #[serde(default)]
/// pub struct Paging { pub limit: Option<String> }
///
/// #[corten::routes]
/// mod url {
///     #[route("/orders", get = list)]
///     pub struct Orders { #[route(query)] pub paging: super::Paging }
/// }
///
/// async fn list(orders: Params<url::Orders>) -> String {
///     orders.paging.limit.clone().unwrap_or_default()
/// }
/// # fn main() {}
/// ```
///
/// ```compile_fail,E0277
/// use corten::prelude::*;
///
/// #[corten::routes]
/// mod url {
///     #[route("/items/{id}", get = show)]
///     pub struct Item<'a> { pub id: &'a str, #[serde(alias = "t")] pub tab: Option<&'a str> }
/// }
///
/// async fn show(item: Params<url::Item<'_>>) -> String {
///     item.id.clone()
/// }
/// # fn main() {}
/// ```
///
/// A route with a type parameter cannot be named for the check, so in a routes module one with a
/// parameter in its path does not compile, with a message that says so (it never could be routed:
/// the module names each route without arguments):
///
/// ```compile_fail
/// use corten::prelude::*;
///
/// #[corten::routes]
/// mod url {
///     #[route("/items/{id}", get = show)]
///     pub struct Item<T: std::fmt::Display> { pub id: T }
/// }
///
/// async fn show() -> &'static str {
///     "item"
/// }
/// # fn main() {}
/// ```
///
/// A link writes `Some("")` as `?tab=`, which reads back as `None` (a blank form field is no
/// value); and a `#[route(prefix = ..)]` map keeps the last value of a key sent twice, as a
/// `BTreeMap` does. A name that a route's field and its `#[route(query)]` set both write, or a
/// set's prefix that one of the route's names starts with, does not compile:
///
/// ```compile_fail,E0080
/// use std::collections::BTreeMap;
/// use corten::prelude::*;
///
/// #[derive(Default, Route)]
/// #[route(query)]
/// pub struct Filters { #[route(prefix = "f")] pub facets: BTreeMap<String, String> }
///
/// #[derive(Route)]
/// #[route("/items")]
/// pub struct Items { pub from: Option<u32>, #[route(query)] pub filters: Filters }
/// ```
///
/// ```compile_fail,E0080
/// use corten::prelude::*;
///
/// #[derive(Default, Route)]
/// #[route(query)]
/// pub struct Paging { pub page: Option<u32> }
///
/// #[derive(Route)]
/// #[route("/items")]
/// pub struct Items { pub page: Option<u32>, #[route(query)] pub paging: Paging }
/// ```
///
/// ```compile_fail,E0080
/// use corten::prelude::*;
///
/// #[derive(Default, Route)]
/// #[route(query)]
/// pub struct Paging { pub page: Option<u32> }
///
/// #[derive(Default, Route)]
/// #[route(query)]
/// pub struct Filters { pub page: Option<u32>, #[route(query)] pub paging: Paging }
///
/// #[derive(Route)]
/// #[route("/items")]
/// pub struct Items { #[route(query)] pub filters: Filters }
/// ```
pub struct Params<R: FromUrl + ?Sized>(pub R::Params);

impl<R: FromUrl + ?Sized> std::ops::Deref for Params<R> {
    type Target = R::Params;

    fn deref(&self) -> &R::Params {
        &self.0
    }
}

impl<R: FromUrl + ?Sized> std::ops::DerefMut for Params<R> {
    fn deref_mut(&mut self) -> &mut R::Params {
        &mut self.0
    }
}

impl<R: FromUrl + ?Sized, S: Send + Sync> axum::extract::FromRequestParts<S> for Params<R>
where
    R::Path: __private::ReadPath,
    R::Params: Send,
{
    type Rejection = ParamsRejection;

    async fn from_request_parts(parts: &mut axum::http::request::Parts, _: &S) -> Result<Self, ParamsRejection> {
        let path = <R::Path as __private::ReadPath>::read(parts).await?;
        R::__read(path, parts.uri.query().unwrap_or_default()).map(Params).map_err(ParamsRejection::query)
    }
}

/// Why [`Params`] refused a request: a path or query parameter that does not fit its field, a
/// 400 in plain text, as axum's rejections are. Boxed, so a request that fits moves nothing large.
#[derive(Debug)]
pub struct ParamsRejection(Box<axum::response::Response>);

impl ParamsRejection {
    #[cold]
    fn query(error: crate::form::FormError) -> Self {
        use axum::response::IntoResponse as _;
        let status = axum::http::StatusCode::BAD_REQUEST;
        // A parameter followed by text is read here rather than by axum's `Path`: said as axum says it.
        let what = if error.is_in_path() { "Invalid URL" } else { "Failed to deserialize query string" };
        let rejection = crate::form::FormRejection::invalid(status, what, error);
        ParamsRejection(Box::new(rejection.into_response()))
    }

    #[cold]
    fn path(rejection: axum::extract::rejection::PathRejection) -> Self {
        use axum::response::IntoResponse as _;
        ParamsRejection(Box::new(rejection.into_response()))
    }
}

impl axum::response::IntoResponse for ParamsRejection {
    fn into_response(self) -> axum::response::Response {
        *self.0
    }
}

/// A route whose parameters a handler reads back, as [`Params`]. Implemented by
/// `#[derive(Route)]` for a struct with fields. Not a public API.
#[doc(hidden)]
#[diagnostic::on_unimplemented(
    message = "`{Self}` has no parameters for `Params` to read",
    note = "Params<R> reads the parameters of a #[derive(Route)] struct with fields, each owned or a reference (&'a str, &'a [T], &'a T)"
)]
pub trait FromUrl {
    /// The route's fields, owned.
    type Params;
    /// Its path parameters, in the order the path names them ([`__private::ReadPath`]).
    type Path;
    /// The fields, from the path's parameters and the query string.
    fn __read(path: Self::Path, query: &str) -> Result<Self::Params, crate::form::FormError>;
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
///
/// It also knows whether one of its handlers is for `GET`, so [`crate::Req::serves`] counts only
/// routes a redirect can land on.
pub struct Methods<S = ()>(pub(crate) axum::routing::MethodRouter<S>, pub(crate) bool);

impl<S: Clone + Send + Sync + 'static> Methods<S> {
    /// The handlers with the state they take (`State<S>`), for a router without it:
    /// `get(item_json).with_state(source)`.
    pub fn with_state<S2>(self, state: S) -> Methods<S2> {
        Methods(self.0.with_state(state), self.1)
    }

    /// Used by `#[corten::routes]`: a route's handlers for two sets of methods. Not a public API.
    #[doc(hidden)]
    pub fn __merge(self, other: Methods<S>) -> Self {
        Methods(self.0.merge(other.0), self.1 || other.1)
    }

    /// Handlers built here by hand, among them one for `GET`.
    #[cfg(test)]
    pub(crate) fn reads(router: axum::routing::MethodRouter<S>) -> Self {
        Methods(router, true)
    }

    /// Handlers built here by hand, none of them for `GET`.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn writes(router: axum::routing::MethodRouter<S>) -> Self {
        Methods(router, false)
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
                Methods(axum::routing::$method(__private::endpoint(handler, <<<H as crate::answer::Signature<T>>::Out as crate::answer::Answer>::Inputs as crate::answer::Accepts<<H as crate::answer::Signature<T>>::Args>>::SHARED)), stringify!($method) == "get")
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
                    Methods(self.0.$method(__private::endpoint(handler, <<<H as crate::answer::Signature<T>>::Out as crate::answer::Answer>::Inputs as crate::answer::Accepts<<H as crate::answer::Signature<T>>::Args>>::SHARED)), self.1 || stringify!($method) == "get")
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
pub struct Routes(pub(crate) axum::Router, pub(crate) Vec<(&'static str, bool)>);

impl Routes {
    /// No routes.
    pub fn new() -> Self {
        Routes::default()
    }

    /// Used by `#[corten::routes]`: `R`'s handlers at its path. Not a public API.
    #[doc(hidden)]
    pub fn __route<R: Route>(self, methods: Methods) -> Self {
        let mut paths = self.1;
        paths.push((R::WRITTEN, methods.1));
        Routes(self.0.route(R::PATH, with_suffixes::<R, ()>(methods.0)), paths)
    }
}

/// Whether `written` is `path` with text after some of its `{name}` parameters (in their segment),
/// as `#[derive(Route)]` writes the two.
const fn written_fits(path: &str, written: &str) -> bool {
    let (p, w) = (path.as_bytes(), written.as_bytes());
    let (mut i, mut j) = (0, 0);
    // Just past a `{name}` (not a `{*rest}`, which nothing follows), and inside a `{*..}`.
    let (mut closed, mut rest) = (false, false);
    while i < p.len() {
        if j >= w.len() {
            return false;
        }
        if closed && p[i] == b'/' && w[j] != b'/' {
            // Text after a parameter, up to the segment's end.
            if !plain(w[j]) {
                return false;
            }
            j += 1;
            continue;
        }
        if p[i] != w[j] {
            return false;
        }
        if p[i] == b'*' && i > 0 && p[i - 1] == b'{' {
            rest = true;
        }
        closed = p[i] == b'}' && !rest;
        if p[i] == b'}' {
            rest = false;
        }
        i += 1;
        j += 1;
    }
    // Text after the last parameter, to the end.
    while closed && j < w.len() {
        if !plain(w[j]) {
            return false;
        }
        j += 1;
    }
    j == w.len()
}

/// Whether routes written `a` and `b` are one path to the router but not the same route: each
/// segment the same text or the same kind of parameter (`{name}`, `{*rest}`, whatever its name),
/// and the text after some parameter different (`/feeds/{name}.rss` and `/feeds/{id}.atom`).
/// Compares in place, with nothing allocated.
pub(crate) fn clash(a: &str, b: &str) -> bool {
    let (mut a, mut b) = (a.split('/'), b.split('/'));
    let mut differ = false;
    loop {
        match (a.next(), b.next()) {
            (None, None) => return differ,
            (Some(x), Some(y)) => match (x.strip_prefix('{'), y.strip_prefix('{')) {
                (Some(x), Some(y)) => {
                    if x.starts_with('*') != y.starts_with('*') {
                        return false;
                    }
                    differ |= text_after(x) != text_after(y);
                }
                (None, None) if x == y => {}
                _ => return false,
            },
            _ => return false,
        }
    }
}

/// The text after a parameter's `}` in its segment (`.xml` for `page}.xml`).
fn text_after(segment: &str) -> &str {
    segment.find('}').map_or("", |n| &segment[n + 1..])
}

/// Two routes of the app (in any module, area or API) that differ only in the text after a
/// parameter, which axum matches as one path: a startup panic naming both, since only one could
/// be reached (a `#[corten::routes]` module already refuses them when it compiles). Routes that
/// differ only in their parameters' names are left to the router, which refuses them itself. So
/// is a route with text beside a catch-all where it has its parameter, which axum would refuse
/// naming the path without the text. Only a route with text after a parameter can clash, so each
/// of those is compared with the others, and an app with none compares nothing.
pub(crate) fn assert_no_clash(written: &[&str]) {
    for (i, a) in written.iter().enumerate().filter(|(_, a)| has_text_after(a)) {
        for other in written[..i].iter().chain(&written[i + 1..]) {
            check(other, a);
        }
    }
}

/// Refuses `a` beside `b`, one of them having text after a parameter: the two differ only in that text,
/// or one has a parameter where the other has its catch-all (`/{name}.txt` and `/{*page}`), which
/// the router would refuse naming `/{name}`.
fn check(a: &str, b: &str) {
    if clash(a, b) {
        refuse(a, b);
    }
    if (a.contains("{*") || b.contains("{*")) && (beside_catch_all(a, b) || beside_catch_all(b, a)) {
        panic!(
            "corten: the routes {a:?} and {b:?} have a parameter where the other has its catch-all, which the router cannot hold together: give the parameter's route a segment of its own"
        )
    }
}

/// Whether `path` has a parameter where `other` has its catch-all, after segments the router
/// matches alike (the same text, or a parameter in each, with or without text after it).
pub(crate) fn beside_catch_all(path: &str, other: &str) -> bool {
    let param = |s: &str| s.starts_with('{');
    let mut ours = path.split('/');
    for theirs in other.split('/') {
        let Some(mine) = ours.next() else { return false };
        if theirs.starts_with("{*") {
            return param(mine) && !mine.starts_with("{*");
        }
        if !(mine == theirs || param(mine) && param(theirs)) {
            return false;
        }
    }
    false
}

/// [`assert_no_clash`] for one route `new`, as an area or an API adds it: against every earlier
/// route when `new` has text after a parameter, else against the earlier ones that have
/// (`with_text`).
pub(crate) fn assert_no_clash_with(earlier: &[&str], with_text: &[&str], new: &str) {
    let against = if has_text_after(new) { earlier } else { with_text };
    for other in against {
        check(other, new);
    }
}

/// Whether a route written `written` has text after one of its parameters.
pub(crate) fn has_text_after(written: &str) -> bool {
    written.split('/').any(|segment| segment.starts_with('{') && !text_after(segment).is_empty())
}

fn refuse(a: &str, b: &str) -> ! {
    panic!(
        "corten: the routes {a:?} and {b:?} differ only in the text after a parameter, which the router does not tell apart: give one a segment of its own"
    )
}

/// `methods` for `R`'s path: when a parameter of it is followed by text (`{page}.xml`), a request
/// whose segment does not end in that text is a 404 before a handler runs, whatever its method. The
/// route still takes every path of its shape from the router, so another route (a `/{*page}`
/// catch-all) does not answer `/sitemap/about` either, and the 404 has the route's `MatchedPath`.
/// A route with none is `methods` as it is.
///
/// A `WRITTEN` path that is not `PATH` with text after some parameters does not build:
///
/// ```compile_fail,E0080
/// # use corten::prelude::*;
/// #[derive(Route)]
/// #[route("/feeds")]
/// struct Feeds;
///
/// struct Feed;
/// impl corten::Route for Feed {
///     const PATH: &'static str = "/feeds/{name}";
///     const WRITTEN: &'static str = "/feeds/{name}.x/extra";
///     fn write_href(&self, out: &mut String, _: bool) {
///         out.push_str("/feeds/a.x/extra");
///     }
/// }
///
/// let app = App::new(Routes::new()).area::<Feeds>(|feeds| feeds.route::<Feed>(get(|| async { "feed" })));
/// # let _ = app.router();
/// ```
///
/// (with `WRITTEN` `"/feeds/{name}.x"` it builds:)
///
/// ```
/// # use corten::prelude::*;
/// #[derive(Route)]
/// #[route("/feeds")]
/// struct Feeds;
///
/// struct Feed;
/// impl corten::Route for Feed {
///     const PATH: &'static str = "/feeds/{name}";
///     const WRITTEN: &'static str = "/feeds/{name}.x";
///     fn write_href(&self, out: &mut String, _: bool) {
///         out.push_str("/feeds/a.x");
///     }
/// }
///
/// let app = App::new(Routes::new()).area::<Feeds>(|feeds| feeds.route::<Feed>(get(|| async { "feed" })));
/// # let _ = app.router();
/// ```
pub(crate) fn with_suffixes<R: Route + ?Sized, S: Clone + Send + Sync + 'static>(
    methods: axum::routing::MethodRouter<S>,
) -> axum::routing::MethodRouter<S> {
    const {
        assert!(
            written_fits(R::PATH, R::WRITTEN),
            "corten: a route's WRITTEN path is its PATH with text after some parameters (`/sitemap/{{page}}.xml` for `/sitemap/{{page}}`)"
        )
    };
    if R::WRITTEN == R::PATH {
        return methods;
    }
    methods.layer(Suffixed { written: R::WRITTEN, inner: () })
}

/// [`with_suffixes`]'s layer and service: a 404 unless the path ends each segment as `written`
/// does. A service of its own rather than `middleware::from_fn`, which boxes a `Next` and a future
/// of its own on each request (the router still boxes the layered route, as it does any layer).
#[derive(Clone)]
struct Suffixed<S> {
    written: &'static str,
    inner: S,
}

impl<S> tower_layer::Layer<S> for Suffixed<()> {
    type Service = Suffixed<S>;

    fn layer(&self, inner: S) -> Suffixed<S> {
        Suffixed { written: self.written, inner }
    }
}

impl<S> tower_service::Service<axum::extract::Request> for Suffixed<S>
where
    S: tower_service::Service<axum::extract::Request, Response = axum::response::Response>,
{
    type Response = axum::response::Response;
    type Error = S::Error;
    type Future = futures_util::future::Either<std::future::Ready<Result<Self::Response, S::Error>>, S::Future>;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), S::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: axum::extract::Request) -> Self::Future {
        if routes_path(self.written, req.uri().path()) {
            futures_util::future::Either::Right(self.inner.call(req))
        } else {
            let missing = axum::response::IntoResponse::into_response(axum::http::StatusCode::NOT_FOUND);
            futures_util::future::Either::Left(std::future::ready(Ok(missing)))
        }
    }
}

/// The crate's tests build routers by hand, to test what `App` does with any response.
#[cfg(test)]
impl From<axum::Router> for Routes {
    fn from(router: axum::Router) -> Self {
        Routes(router, Vec::new())
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
    // read, a module's `caller = ..` it does not take (`R`, `()` for none), the route's own
    // `Params` when its path has a parameter (`P`, `()` for none), or rows other than the ones the
    // route names (`D`: `rows = Level` on its `get`, `NotAFeed` otherwise).
    // `super::get` and the others are the same check without a caller.
    macro_rules! methods {
        ($($method:ident),*) => {
            $(
                #[inline]
                pub fn $method<H, T, S, R, I, P, J, D>(handler: H) -> Methods<S>
                where
                    H: RouteHandler<T, S> + crate::answer::Signature<T>,
                    <H as RouteHandler<T, S>>::Handler: Handler<T, S>,
                    <<H as RouteHandler<T, S>>::Handler as Handler<T, S>>::Future: Unpin,
                    <H as crate::answer::Signature<T>>::Out: crate::answer::Answer + 'static,
                    <<H as crate::answer::Signature<T>>::Out as crate::answer::Answer>::Rows: RowsAre<D>,
                    <<H as crate::answer::Signature<T>>::Out as crate::answer::Answer>::Inputs:
                        crate::answer::Accepts<<H as crate::answer::Signature<T>>::Args>,
                    <H as crate::answer::Signature<T>>::Args: crate::answer::Holds<R, I> + crate::answer::Holds<P, J>,
                    T: 'static,
                    S: Clone + Send + Sync + 'static,
                {
                    Methods(axum::routing::$method(endpoint(handler, <<<H as crate::answer::Signature<T>>::Out as crate::answer::Answer>::Inputs as crate::answer::Accepts<<H as crate::answer::Signature<T>>::Args>>::SHARED)), matches!(stringify!($method), "get" | "any"))
                }
            )*

        };
    }
    // `any`: every method, those the app never names included (a relay to another server).
    methods!(get, post, put, delete, patch, head, options, trace, any);

    /// A handler's rows (`Self`, [`crate::answer::Answer::Rows`]) are the ones its route names
    /// (`D`).
    #[diagnostic::on_unimplemented(
        message = "the route's rows are {D}, and this handler answers with rows {Self}",
        label = "this handler's rows",
        note = "a route whose get serves a feed names its rows, #[route(\"/feed/levels\", get = levels, rows = Level)] for a handler that returns FeedResponse<Level>; a route that serves no feed (NotAFeed) names none"
    )]
    pub trait RowsAre<D> {}
    impl<T> RowsAre<T> for T {}

    /// A route whose `get` serves a feed of `Row`s (`rows = Level`): what a script's
    /// `FeedUrl<Level>` parameter takes.
    #[diagnostic::on_unimplemented(
        message = "{Self} is not the URL of a feed of {Row} rows",
        label = "the handler's JSDoc says FeedUrl<{Row}>, and this route names other rows or none",
        note = "a feed's route names its rows, #[route(\"/feed/levels\", get = levels, rows = Level)], and the script's parameter names the same type, @param {{FeedUrl<Level>}} feed"
    )]
    pub trait FeedRows<Row> {}

    pub use crate::form::de::{sorted_pairs, Pair};
    use crate::form::FormError;

    /// A path parameter followed by text in its segment (`{page}.xml`), read without the text,
    /// then as its type. The router has checked the text is there.
    fn unsuffixed<T: serde::de::DeserializeOwned>(text: &str, suffix: &str) -> Result<T, FormError> {
        let value =
            text.strip_suffix(suffix).ok_or_else(|| FormError::in_path(format!("`{text}` does not end in `{suffix}`")))?;
        crate::form::de::in_path(value).map_err(|e| {
            // As axum says it: a primitive's name, never the app's own type path.
            let ty = std::any::type_name::<T>();
            FormError::in_path(if ty.contains("::") {
                format!("{e}")
            } else {
                format!("Cannot parse `{value}` to a `{ty}`: {e}")
            })
        })
    }

    /// `T`, named with a lifetime of the `FromUrl` impl, so a bound on it (that a field reads
    /// back) is checked where `Params` is used, not where the route is declared: a link needs
    /// only `Display`.
    pub struct Later<'a, T: ?Sized>(std::marker::PhantomData<Mark<'a, T>>);

    /// Names `'a` and `T` without requiring `T: 'a` (`T` may borrow for another lifetime).
    type Mark<'a, T> = fn(&'a ()) -> *const T;

    /// A field `Params` has no owned form for: a borrowed query set (`ItemQuery<'a>`), `&'a mut T`.
    #[diagnostic::on_unimplemented(
        message = "`Params` cannot read a route field borrowed this way: `{Self}`",
        note = "a field Params reads back is owned, a reference (&'a str, &'a [T], &'a T) or a Cow<'a, T>: make a #[route(query)] set an owned struct"
    )]
    pub trait Owned {}

    /// A `#[serde(..)]` key on a route that `Params` does not follow, by name.
    #[allow(non_camel_case_types)]
    pub mod serde_key {
        pub struct alias;
        pub struct deserialize_with;
        pub struct with;
        pub struct flatten;
        pub struct skip;
        pub struct skip_deserializing;
        pub struct borrow;
        pub struct default;
        pub struct deny_unknown_fields;
        pub struct from;
        pub struct try_from;
        pub struct transparent;
    }

    /// Never implemented: a route or query set with a serde key `Params` does not follow has no
    /// `Params`.
    #[diagnostic::on_unimplemented(
        message = "`Params` reads a route's parameters by the names its links write, and does not follow this #[serde(..)] key: `{Self}`",
        note = "remove the key from the route struct, or from the #[route(query)] set that has it"
    )]
    pub trait Followed {}

    /// A query parameter's name as a route or set writes it: a key, a `#[route(prefix)]` family,
    /// or a `#[route(query)]` set's own.
    pub enum Key {
        Name(&'static str),
        Prefix(&'static str),
        Set(&'static [Key]),
        /// A name read and never written (`#[route(read_only)]`): no other parameter may have it,
        /// but a link does not write it.
        ReadOnly(&'static str),
    }

    /// How many names `keys` has, sets flattened; only those a link writes when `written`.
    const fn count(keys: &[Key], written: bool) -> usize {
        let (mut n, mut i) = (0, 0);
        while i < keys.len() {
            n += match &keys[i] {
                Key::Set(inner) => count(inner, written),
                Key::ReadOnly(_) if written => 0,
                _ => 1,
            };
            i += 1;
        }
        n
    }

    /// The `n`th name, sets flattened: `(is a prefix, text)`; of those a link writes when `written`.
    const fn nth(keys: &[Key], mut n: usize, written: bool) -> Option<(bool, &'static str)> {
        let mut i = 0;
        while i < keys.len() {
            match &keys[i] {
                Key::Set(inner) => {
                    let c = count(inner, written);
                    if n < c {
                        return nth(inner, n, written);
                    }
                    n -= c;
                }
                Key::ReadOnly(_) if written => {}
                Key::Name(name) | Key::ReadOnly(name) if n == 0 => return Some((false, *name)),
                Key::Prefix(prefix) if n == 0 => return Some((true, *prefix)),
                _ => n -= 1,
            }
            i += 1;
        }
        None
    }

    const fn starts_with(text: &str, prefix: &str) -> bool {
        let (text, prefix) = (text.as_bytes(), prefix.as_bytes());
        if prefix.len() > text.len() {
            return false;
        }
        let mut i = 0;
        while i < prefix.len() {
            if text[i] != prefix[i] {
                return false;
            }
            i += 1;
        }
        true
    }

    /// Fails to compile a `FromQuery` set whose link and read sets do not write the same names
    /// (a `#[route(read_only)]` name the read set has besides is not written).
    pub const fn same_keys(link: &[Key], read: &[Key]) {
        const fn has(keys: &[Key], prefix: bool, text: &str) -> bool {
            let mut i = 0;
            while let Some((p, t)) = nth(keys, i, true) {
                if p == prefix && t.len() == text.len() && starts_with(t, text) {
                    return true;
                }
                i += 1;
            }
            false
        }
        let total = count(link, true);
        let mut same = total == count(read, true);
        let mut i = 0;
        while same && i < total {
            let (Some((lp, lt)), Some((rp, rt))) = (nth(link, i, true), nth(read, i, true)) else { unreachable!() };
            same = has(read, lp, lt) && has(link, rp, rt);
            i += 1;
        }
        if !same {
            panic!(
                "a FromQuery set's Link and Read write different names: give the two sets the same fields' names and prefixes, and mark a name the Read set only reads #[route(read_only)]"
            );
        }
    }

    /// Fails to compile a route whose parameters, its own and its sets', could be read back as
    /// each other's: two with one name, or a prefix another's name starts with.
    pub const fn check_keys(keys: &[Key]) {
        let total = count(keys, false);
        let mut a = 0;
        while a < total {
            let mut b = a + 1;
            while b < total {
                let (Some((a_prefix, a_text)), Some((b_prefix, b_text))) = (nth(keys, a, false), nth(keys, b, false)) else {
                    unreachable!()
                };
                let clash = match (a_prefix, b_prefix) {
                    (false, false) => starts_with(a_text, b_text) && a_text.len() == b_text.len(),
                    (true, false) => starts_with(b_text, a_text),
                    (false, true) => starts_with(a_text, b_text),
                    (true, true) => starts_with(a_text, b_text) || starts_with(b_text, a_text),
                };
                if clash {
                    panic!("two of this route's query parameters, its own or its #[route(query)] sets', have one name, or a #[route(prefix)] starts another's name, so a parameter read back could be either's");
                }
                b += 1;
            }
            a += 1;
        }
    }

    /// A query parameter read by serde, by its key.
    pub trait Deserialized {
        type T;
        fn field(pairs: &[Pair<'_>], key: &str, default: Option<fn() -> Self::T>) -> Result<Self::T, FormError>;
    }

    /// A path parameter followed by text in its segment (`{page}.xml`), read without the text,
    /// then as its type; through `Later`, so a route used only for links needs only `Display`.
    pub trait Unsuffixed {
        type T;
        fn unsuffixed(text: &str, suffix: &str) -> Result<Self::T, FormError>;
    }

    impl<T: serde::de::DeserializeOwned> Unsuffixed for Later<'_, T> {
        type T = T;

        #[inline]
        fn unsuffixed(text: &str, suffix: &str) -> Result<T, FormError> {
            unsuffixed(text, suffix)
        }
    }

    impl<T: serde::de::DeserializeOwned> Deserialized for Later<'_, T> {
        type T = T;

        #[inline]
        fn field(pairs: &[Pair<'_>], key: &str, default: Option<fn() -> T>) -> Result<T, FormError> {
            crate::form::de::field(pairs, key, default)
        }
    }

    /// A `#[route(query)]` set read back for `Params`: each of its fields by the key its links
    /// write, as a route reads its own (a prefix field by its prefix, a set inside it as a set).
    /// `'c` names the `FromUrl` impl's lifetime, so a field that cannot be read is an error where
    /// `Params` is used, not where the set is declared.
    #[diagnostic::on_unimplemented(
        message = "`Params` cannot read this #[route(query)] set back: `{Self}`",
        note = "a set Params reads is `#[derive(Route)] #[route(query)]` with no lifetime parameters, its fields owning their values (String, not &'a str), or one that implements corten::route::FromQuery with such a set as its Read"
    )]
    pub trait ReadQuery<'c>: Sized {
        fn read_query(pairs: &[Pair<'_>]) -> Result<Self, FormError>;
    }

    impl<'c, T: super::FromQuery> ReadQuery<'c> for T
    where
        T::Read: ReadQuery<'c>,
    {
        #[inline]
        fn read_query(pairs: &[Pair<'_>]) -> Result<Self, FormError> {
            T::from_query(T::Read::read_query(pairs)?)
        }
    }

    /// A `#[route(prefix = "dim.")]` field: a `BTreeMap` or a `Vec` of pairs.
    pub trait Prefixed {
        type T;
        fn prefixed(pairs: &[Pair<'_>], prefix: &str) -> Result<Self::T, FormError>;
    }

    impl<K: serde::de::DeserializeOwned + Ord, V: serde::de::DeserializeOwned> Prefixed
        for Later<'_, std::collections::BTreeMap<K, V>>
    {
        type T = std::collections::BTreeMap<K, V>;

        fn prefixed(pairs: &[Pair<'_>], prefix: &str) -> Result<Self::T, FormError> {
            crate::form::de::prefixed(pairs, prefix)
                .filter_map(|(k, v)| crate::form::de::entry(prefix, k, v).transpose())
                .collect()
        }
    }

    impl<K: serde::de::DeserializeOwned, V: serde::de::DeserializeOwned> Prefixed for Later<'_, Vec<(K, V)>> {
        type T = Vec<(K, V)>;

        fn prefixed(pairs: &[Pair<'_>], prefix: &str) -> Result<Self::T, FormError> {
            crate::form::de::prefixed(pairs, prefix)
                .filter_map(|(k, v)| crate::form::de::entry(prefix, k, v).transpose())
                .collect()
        }
    }

    /// A route's path parameters, read for [`super::Params`]: `()` when its path has none.
    pub trait ReadPath: Sized + Send {
        fn read(
            parts: &mut axum::http::request::Parts,
        ) -> impl std::future::Future<Output = Result<Self, super::ParamsRejection>> + Send;
    }

    impl ReadPath for () {
        #[inline]
        fn read(
            _: &mut axum::http::request::Parts,
        ) -> impl std::future::Future<Output = Result<Self, super::ParamsRejection>> + Send {
            std::future::ready(Ok(()))
        }
    }

    /// The path's parameters as a tuple, in the order the path names them.
    pub struct InPath<T>(pub T);

    impl<T: serde::de::DeserializeOwned + Send> ReadPath for InPath<T> {
        #[inline]
        async fn read(parts: &mut axum::http::request::Parts) -> Result<Self, super::ParamsRejection> {
            use axum::extract::FromRequestParts as _;
            match axum::extract::Path::<T>::from_request_parts(parts, &()).await {
                Ok(axum::extract::Path(value)) => Ok(InPath(value)),
                Err(rejection) => Err(super::ParamsRejection::path(rejection)),
            }
        }
    }

    /// The handler with its answer in its type, so a JSON route's failures are marked as JSON, and
    /// whether the answer is the same for everyone (`Public`, known when the crate compiles).
    #[inline]
    pub(crate) fn endpoint<H, T, S>(
        handler: H,
        shared: bool,
    ) -> crate::answer::Endpoint<H::Handler, <H as crate::answer::Signature<T>>::Out>
    where
        H: RouteHandler<T, S> + crate::answer::Signature<T>,
    {
        #[cfg(not(feature = "zones"))]
        let _ = shared;
        crate::answer::Endpoint {
            handler: handler.into_handler(),
            answer: std::marker::PhantomData,
            #[cfg(feature = "zones")]
            shared,
        }
    }

    /// A handler a route names: an async function whose arguments a request fills (extractors,
    /// `Req`, `State<..>` of the routes module's state) and whose result is a response.
    #[diagnostic::on_unimplemented(
        message = "`{Self}` cannot serve this route",
        label = "not a handler for these routes",
        note = "a handler is an async fn whose arguments are extractors (Req, Params, Form, ..), with a State<T> only for the module's #[corten::routes(state = T)], and whose result is a response"
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

    /// The derive names the primitive `str` by its full path, so an app's type of that name (or a
    /// raw identifier) is not mistaken for it.
    #[test]
    fn url_value_derives_for_an_enum_named_str() {
        #[allow(non_camel_case_types)]
        #[derive(corten::UrlValue, Clone, Copy)]
        enum r#str {
            r#type,
            Sand,
        }
        assert_eq!(r#str::r#type.as_ref(), "type");
        assert_eq!(r#str::Sand.to_string(), "Sand");
    }

    /// Each word `UrlValue` writes is the one serde reads back as the same variant, under every
    /// rule, through the reader `Params` uses.
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
