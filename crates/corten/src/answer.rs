//! What a route answers, named by its handler's return type: [`HtmlResult`] for a page, a patch or
//! a redirect, [`JsonResult<T>`] for JSON, and [`Public`] around either for an answer that is the
//! same for everyone. The type decides how the route's failures are answered (the error page, or
//! JSON) and what the handler may read, so `#[corten::routes]` checks both when the crate
//! compiles:
//!
#![cfg_attr(feature = "json", doc = "```")]
#![cfg_attr(not(feature = "json"), doc = "```ignore")]
//! use corten::prelude::*;
//!
//! #[corten::routes]
//! mod url {
//!     #[route("/", get = home)]
//!     pub struct Home;
//!
//!     #[route("/api/price/{id}", get = price)]
//!     pub struct Price { pub id: u32 }
//! }
//!
//! async fn home(req: Req) -> HtmlResult {
//!     req.page(html! { <html><head><title>"Home"</title><Corten /></head><body><main>"Home"</main></body></html> })
//! }
//!
//! #[derive(ToJson)]
//! struct Quote { id: u32, cents: u32 }
//!
//! // A path that does not parse is a JSON 400, whatever the client accepts.
//! async fn price(Path(id): Path<u32>) -> JsonResult<Quote> {
//!     Ok(Json(Quote { id, cents: 900 }))
//! }
//! # let _ = App::new(url::router()).router();
//! ```
//!
//! A handler that hides what it answers does not compile:
//!
//! ```compile_fail,E0277
//! use corten::{axum::response::Response, prelude::*};
//!
//! #[corten::routes]
//! mod url {
//!     #[route("/", get = home)]
//!     pub struct Home;
//! }
//!
//! async fn home() -> Response {
//!     Response::default()
//! }
//! # fn main() { let _ = url::router(); }
//! ```

use std::{
    future::Future,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use axum::{
    extract::{Request, State},
    handler::Handler,
    http::{Method, StatusCode, Uri},
    response::{IntoResponse, Response},
};

use crate::{cache::Cache, Error, Req, Signal};

/// How a route's failures are answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// By the request's mode: the app's error page for a browser, JSON for a data client
    /// (`Accept: application/json`), the status line for anything else.
    Html,
    /// JSON, `{"error":{"status","message"}}`, whatever the client accepts.
    Json,
}

/// What a handler named in `#[corten::routes]` returns: the type says how the route's failures are
/// answered ([`Answer::FORMAT`]) and what the handler may read ([`Answer::Inputs`]). Implemented
/// for [`HtmlResult`], [`JsonResult<T>`], `Result<JsonResponse, JsonError>`, [`Public`], [`NoStore`], text, a bare `StatusCode`,
/// [`LiveResponse`], feeds and [`crate::seo::File`]; a crate may add its own.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not an answer a route can name",
    label = "the handler returns this",
    note = "return `HtmlResult` (a page, a patch, a redirect), `JsonResult<T>`, or `Public<..>` of one; `Response` and `impl IntoResponse` hide what the route answers"
)]
pub trait Answer: IntoResponse {
    /// How the route's failures are answered.
    const FORMAT: Format;
    /// What the handler may read: [`AnyInputs`], or [`AnonymousInputs`] for an answer that is
    /// the same for everyone.
    type Inputs;
}

/// A page, a patch or a redirect, as a whole response (status and headers too): what
/// [`Req::page`], [`Req::patch`] and [`Req::redirect`] give. A cookie is
/// [`HtmlResponse::with_cookie`]; it derefs to the response for any other header of the
/// handler's own (`res.headers_mut().insert(..)`).
///
/// `For` is who the answer is for, read from where it goes: [`ForOne`], the visitor who asked
/// (an [`HtmlResult`]), or [`ForAll`], inside a [`Public`]. The `Req` methods that make one
/// take it from there, so a handler never names it.
pub struct HtmlResponse<For = ForOne>(pub(crate) Response, PhantomData<For>);

/// An answer for the visitor who asked: it may set a cookie. What [`HtmlResult`] holds.
pub struct ForOne;

/// An answer for everyone, inside a [`Public`]: a cache gives it to anyone, so it sets no
/// cookie, and [`HtmlResponse::with_cookie`] there does not compile.
pub struct ForAll;

/// Who an answer that sets a cookie can be for: [`ForOne`] alone.
#[diagnostic::on_unimplemented(
    message = "a `Public` answer is the same for everyone, so it cannot set a cookie",
    label = "this answer is `Public`",
    note = "return a private answer (`HtmlResult`) to set a cookie; a cache would give the cookie to everyone"
)]
pub trait SetsCookies: sealed::Sealed {}

impl SetsCookies for ForOne {}

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::ForOne {}
}

impl<For> HtmlResponse<For> {
    pub(crate) fn new(res: Response) -> Self {
        HtmlResponse(res, PhantomData)
    }
}

impl<For: SetsCookies> HtmlResponse<For> {
    /// The answer with a `Set-Cookie` header, built by [`SetCookie`](crate::security::cookie::SetCookie)
    /// so its safe flags are the default: `Ok(req.redirect(&url::Home)?.with_cookie(SetCookie::new("session", &token)))`.
    /// Each call adds one; an answer that sets a cookie is never stored by a shared cache. This is
    /// the way to set one: never append a `Set-Cookie` header by hand.
    ///
    /// ```
    /// use corten::{prelude::*, security::cookie::SetCookie};
    ///
    /// #[corten::routes]
    /// mod url {
    ///     #[route("/", get = home)]
    ///     pub struct Home;
    ///
    ///     #[route("/sign-out", post = sign_out)]
    ///     pub struct SignOut;
    /// }
    ///
    /// # async fn home(req: Req) -> HtmlResult {
    /// #     req.page(html! { <html><head><title>"Home"</title><Corten /></head><body><main>"Home"</main></body></html> })
    /// # }
    /// async fn sign_out(req: Req) -> HtmlResult {
    ///     Ok(req.redirect(&url::Home)?.with_cookie(SetCookie::remove("session")))
    /// }
    /// # let _ = App::new(url::router()).router();
    /// ```
    ///
    /// A [`Public`] answer is the same for everyone, so setting a cookie on one does not compile:
    ///
    /// ```compile_fail,E0277
    /// use std::time::Duration;
    /// use corten::{prelude::*, security::cookie::SetCookie};
    ///
    /// #[corten::routes]
    /// mod url {
    ///     #[route("/", get = home)]
    ///     pub struct Home;
    /// }
    ///
    /// async fn home(req: Req) -> Public<HtmlResult> {
    ///     let page = req.page(html! { <html><head><title>"Home"</title><Corten /></head><body><main>"Home"</main></body></html> });
    ///     Public::new(Duration::from_secs(60), page.map(|page| page.with_cookie(SetCookie::new("seen", "1"))))
    /// }
    /// # fn main() { let _ = url::router(); }
    /// ```
    #[must_use]
    pub fn with_cookie(mut self, cookie: crate::security::cookie::SetCookie) -> Self {
        self.0.headers_mut().append(axum::http::header::SET_COOKIE, cookie.header());
        self
    }
}

impl<For> IntoResponse for HtmlResponse<For> {
    fn into_response(self) -> Response {
        self.0
    }
}

impl<For> Deref for HtmlResponse<For> {
    type Target = Response;

    fn deref(&self) -> &Response {
        &self.0
    }
}

impl<For> DerefMut for HtmlResponse<For> {
    fn deref_mut(&mut self) -> &mut Response {
        &mut self.0
    }
}

impl Answer for HtmlResponse {
    const FORMAT: Format = Format::Html;
    type Inputs = AnyInputs;
}

/// An answer that can be [`Public`], and what it is there: the same answer, made for everyone.
/// An [`HtmlResult`] becomes a `Result<HtmlResponse<ForAll>>`, so the `Req` methods that make
/// it make it for everyone and it cannot set a cookie; an answer that cannot set one (text, JSON,
/// a [`Relay`]) stays itself. An answer type of the app's own implements it (with
/// [`ForEveryone`] for `ForAll`) only when it sets no cookie.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be `Public`",
    note = "a `Public` answer is a page, a patch or a redirect (`HtmlResult`), `JsonResult<T>`, text, a status, a `Relay`, a live stream or a feed"
)]
pub trait Shareable {
    /// The answer as [`Public::new`] takes it.
    type ForAll: ForEveryone<Named = Self>;
}

/// An answer made for everyone, as [`Public::new`] takes it, and the answer type a handler
/// names for it (`Named`): `Result<HtmlResponse<ForAll>>` is named `HtmlResult`. Lets
/// `Public::new(age, answer)` find its type from `answer` where no return type says it (a
/// closure).
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not an answer made for everyone, so it cannot be `Public`",
    note = "a page made for one visitor (`HtmlResult`) cannot be: make it in the handler that returns `Public<HtmlResult>`, or have a helper return `corten::Result<HtmlResponse<For>>` for any `For`; a `Public` answer is a page, a patch or a redirect, `JsonResult<T>`, text, a status, a `Relay`, a live stream or a feed"
)]
pub trait ForEveryone: IntoResponse {
    /// The answer type a handler names: `Public<Self::Named>`.
    type Named: Shareable;

    /// Itself, as the type `Public<Self::Named>` holds (the same type, named another way).
    #[doc(hidden)]
    fn for_all(self) -> <Self::Named as Shareable>::ForAll;
}

impl ForEveryone for HtmlResponse<ForAll> {
    type Named = HtmlResponse;

    fn for_all(self) -> Self {
        self
    }
}

impl<A: ForEveryone> ForEveryone for Result<A, Error> {
    type Named = Result<A::Named, Error>;

    fn for_all(self) -> Result<<A::Named as Shareable>::ForAll, Error> {
        self.map(A::for_all)
    }
}

#[cfg(feature = "json")]
impl<A: ForEveryone> ForEveryone for std::result::Result<A, JsonError> {
    type Named = std::result::Result<A::Named, JsonError>;

    fn for_all(self) -> std::result::Result<<A::Named as Shareable>::ForAll, JsonError> {
        self.map(A::for_all)
    }
}

/// An answer that is itself everywhere: it sets no cookie.
macro_rules! for_everyone {
    ($($ty:ty),* $(,)?) => {$(
        impl ForEveryone for $ty {
            type Named = Self;

            fn for_all(self) -> Self {
                self
            }
        }
        impl Shareable for $ty {
            type ForAll = Self;
        }
    )*};
}

for_everyone!(&'static str, String, StatusCode, Relay, LiveResponse);

#[cfg(feature = "feed")]
for_everyone!(corten_feed::FeedResponse);

impl Shareable for HtmlResponse {
    type ForAll = HtmlResponse<ForAll>;
}

impl<A: Shareable> Shareable for Result<A, Error> {
    type ForAll = Result<A::ForAll, Error>;
}

#[cfg(feature = "json")]
impl<A: Shareable> Shareable for std::result::Result<A, JsonError> {
    type ForAll = std::result::Result<A::ForAll, JsonError>;
}

#[cfg(feature = "json")]
impl<T> Shareable for crate::Json<T>
where
    crate::Json<T>: IntoResponse,
{
    type ForAll = Self;
}

#[cfg(feature = "json")]
impl<T> ForEveryone for crate::Json<T>
where
    crate::Json<T>: IntoResponse,
{
    type Named = Self;

    fn for_all(self) -> Self {
        self
    }
}

/// What a page handler returns: `async fn item(req: Req) -> HtmlResult`. A failure is the app's
/// error page for a browser.
pub type HtmlResult = crate::Result<HtmlResponse>;

/// What a JSON handler returns: `async fn price(..) -> JsonResult<Quote>`. Every failure of the
/// route, a path or body that does not parse included, is JSON. Its error is a [`JsonError`], so
/// the handler `?`s its input's field errors (a 422 with the fields) next to any other error.
#[cfg(feature = "json")]
pub type JsonResult<T> = std::result::Result<crate::Json<T>, JsonError>;

/// A JSON answer with a status or a cookie of its own, made from [`crate::Json`]:
/// `Json(started).with_status(StatusCode::ACCEPTED)`, `Json(session).with_cookie(cookie)`. Its
/// handler returns `Result<JsonResponse, JsonError>` where one that answers `200` with no cookie
/// returns [`JsonResult<T>`]. The value is written when it is made, with the same headers as any
/// JSON answer; it derefs to the response for any other header of the handler's own.
///
/// A status here is the app's own answer, body and all: `503 {"error":"No forecast yet"}` reaches
/// the client as written. A failure (`Err`) is answered as every failure of a JSON route is,
/// `{"error":{"status","message"}}`.
///
/// ```
/// use corten::{axum::http::StatusCode, json::Raw, prelude::*, security::cookie::SetCookie, JsonError, JsonResponse};
///
/// #[derive(serde::Serialize, ToJson)]
/// struct Started { ok: bool }
///
/// #[derive(serde::Serialize, ToJson)]
/// struct Problem { error: &'static str }
///
/// async fn refresh() -> Result<JsonResponse, JsonError> {
///     Ok(Json(Started { ok: true }).with_status(StatusCode::ACCEPTED))
/// }
///
/// async fn capacity(State(forecast): State<Option<Raw>>) -> Result<JsonResponse, JsonError> {
///     Ok(match forecast {
///         Some(doc) => Json(doc).into(),
///         None => Json(Problem { error: "No capacity forecast yet" }).with_status(StatusCode::SERVICE_UNAVAILABLE),
///     })
/// }
///
/// async fn sign_in() -> Result<JsonResponse, JsonError> {
///     let token = "c2Vzc2lvbg";
///     Ok(Json(Started { ok: true }).with_cookie(SetCookie::try_new("session", token)?))
/// }
/// # let _ = (refresh, capacity, sign_in);
/// ```
///
/// A cookie is a [`SetCookie`](crate::security::cookie::SetCookie), so its safe flags are the
/// default; text does not compile:
///
/// ```compile_fail,E0308
/// use corten::prelude::*;
///
/// let _ = Json(1).with_cookie("session=abc");
/// ```
///
/// It answers the visitor who asked, so it is never [`Public`]: a cache would give its cookie to
/// everyone. A public JSON answer is a [`JsonResult<T>`], held bytes included
/// (`Public<JsonResult<Raw>>`).
///
/// ```compile_fail,E0277
/// use std::time::Duration;
/// use corten::{prelude::*, JsonError, JsonResponse};
///
/// async fn rates() -> Public<Result<JsonResponse, JsonError>> {
///     let answer: Result<JsonResponse, JsonError> = Ok(Json(1).into());
///     Public::new(Duration::from_secs(60), answer)
/// }
/// ```
#[cfg(feature = "json")]
pub struct JsonResponse(Response);

#[cfg(feature = "json")]
impl JsonResponse {
    /// The answer with `status`. A status that has no body (`204`, `304`, a `1xx`) is not one
    /// for a JSON answer: a debug build panics, where a bare `StatusCode` answer is the way.
    #[must_use]
    #[track_caller]
    pub fn with_status(mut self, status: StatusCode) -> Self {
        debug_assert!(
            !(status.is_informational()
                || matches!(status, StatusCode::NO_CONTENT | StatusCode::RESET_CONTENT | StatusCode::NOT_MODIFIED)),
            "corten: {status} has no body, so it is not a JSON answer's status; answer with the StatusCode itself"
        );
        *self.0.status_mut() = status;
        self
    }

    /// The answer with a `Set-Cookie` header, as [`HtmlResponse::with_cookie`]: each call adds
    /// one, and an answer that sets a cookie is never stored by a shared cache. Never append a
    /// `Set-Cookie` header by hand.
    #[must_use]
    pub fn with_cookie(mut self, cookie: crate::security::cookie::SetCookie) -> Self {
        self.0.headers_mut().append(axum::http::header::SET_COOKIE, cookie.header());
        self
    }
}

#[cfg(feature = "json")]
impl<T: crate::json::ToJson> crate::Json<T> {
    /// This value answered with `status`: [`JsonResponse::with_status`].
    #[must_use]
    #[track_caller]
    pub fn with_status(self, status: StatusCode) -> JsonResponse {
        JsonResponse::from(self).with_status(status)
    }

    /// This value answered with a cookie: [`JsonResponse::with_cookie`].
    #[must_use]
    pub fn with_cookie(self, cookie: crate::security::cookie::SetCookie) -> JsonResponse {
        JsonResponse::from(self).with_cookie(cookie)
    }
}

#[cfg(feature = "json")]
impl<T: crate::json::ToJson> From<crate::Json<T>> for JsonResponse {
    fn from(json: crate::Json<T>) -> Self {
        JsonResponse(json.into_response())
    }
}

#[cfg(feature = "json")]
impl IntoResponse for JsonResponse {
    fn into_response(self) -> Response {
        self.0
    }
}

#[cfg(feature = "json")]
impl Deref for JsonResponse {
    type Target = Response;

    fn deref(&self) -> &Response {
        &self.0
    }
}

#[cfg(feature = "json")]
impl DerefMut for JsonResponse {
    fn deref_mut(&mut self) -> &mut Response {
        &mut self.0
    }
}

#[cfg(feature = "json")]
impl Answer for JsonResponse {
    const FORMAT: Format = Format::Json;
    type Inputs = AnyInputs;
}

/// A JSON handler's error: an [`Error`] (`?` on any error, as in a page handler) or the
/// input's field errors, `?` on [`crate::Errors`], answered as the 422 with `fields`. Only a
/// JSON handler takes field errors this way: a page shows its form again with
/// [`crate::Req::invalid`], so `?` on `Errors` there does not compile:
///
/// ```compile_fail,E0277
/// use corten::prelude::*;
///
/// fn check(email: &str) -> Result<(), Errors> {
///     let mut errors = Errors::new();
///     if !email.contains('@') {
///         errors.add_form("Enter an email address.");
///     }
///     errors.into_result(())
/// }
///
/// async fn sign_up(req: Req) -> HtmlResult {
///     check("ada")?;
///     req.page(html! { <html><head><title>"Hi"</title><Corten /></head><body><main>"Hi"</main></body></html> })
/// }
/// ```
#[cfg(feature = "json")]
#[derive(Debug)]
pub enum JsonError {
    /// Any failure, answered as every failure of a JSON route is.
    Error(Error),
    /// The input's field errors: 422, `{"error":{..,"fields":{..}}}`.
    Fields(crate::Errors),
}

#[cfg(feature = "json")]
impl<E: Into<Error>> From<E> for JsonError {
    fn from(error: E) -> Self {
        JsonError::Error(error.into())
    }
}

#[cfg(feature = "json")]
impl From<crate::Errors> for JsonError {
    fn from(errors: crate::Errors) -> Self {
        JsonError::Fields(errors)
    }
}

#[cfg(feature = "json")]
impl IntoResponse for JsonError {
    fn into_response(self) -> Response {
        match self {
            JsonError::Error(error) => error.into_response(),
            JsonError::Fields(errors) => errors.into_response(),
        }
    }
}

#[cfg(feature = "json")]
impl<A: Answer> Answer for std::result::Result<A, JsonError> {
    const FORMAT: Format = A::FORMAT;
    type Inputs = A::Inputs;
}

#[cfg(feature = "json")]
impl<T> Answer for crate::Json<T>
where
    crate::Json<T>: IntoResponse,
{
    const FORMAT: Format = Format::Json;
    type Inputs = AnyInputs;
}

impl<A: Answer> Answer for Result<A, Error> {
    const FORMAT: Format = A::FORMAT;
    type Inputs = A::Inputs;
}

impl Answer for &'static str {
    const FORMAT: Format = Format::Html;
    type Inputs = AnyInputs;
}

impl Answer for String {
    const FORMAT: Format = Format::Html;
    type Inputs = AnyInputs;
}

/// A status and no body: `StatusCode::NO_CONTENT` for a write that has nothing to say. An error
/// status is answered as any failure of the route is.
impl Answer for StatusCode {
    const FORMAT: Format = Format::Html;
    type Inputs = AnyInputs;
}

/// Another server's answer, passed on as it came, for a route that relays an upstream (a gateway,
/// a backend for a frontend): its status, its headers less the hop-by-hop ones (`Connection` and
/// the headers it names, `Keep-Alive`, `Proxy-Authenticate`, `Proxy-Authorization`, `TE`,
/// `Trailer`, `Transfer-Encoding`, `Upgrade`), and its body as it arrives. A failure status is
/// passed on with the upstream's body, never replaced by the error page or the JSON error, and
/// the app's security and cache rules still apply to it, except that a `Content-Security-Policy`
/// the upstream sent is kept in place of the app's: a relayed page needs the policy its own HTML
/// was written for (its scripts' nonces and hosts). Without one, the app's applies. Corten does
/// not compress it: its encoding is the upstream's (forward the client's `Accept-Encoding` to
/// have the upstream compress, with an HTTP client that does not decode the body: for reqwest,
/// without its `gzip`, `brotli` and `zstd` features). When the client
/// leaves, the body is dropped, and with it the upstream request. What else to drop
/// (`Set-Cookie`, say) is the route's to decide before it builds the answer.
///
/// ```
/// use corten::{axum::{body::Body, http::{HeaderMap, StatusCode}}, prelude::*, Relay};
///
/// async fn models() -> corten::Result<Relay> {
///     // The upstream's answer, from the app's HTTP client: for reqwest,
///     // `Relay::new(res.status(), res.headers().clone(), Body::from_stream(res.bytes_stream()))`.
///     let (status, headers, body) = (StatusCode::BAD_GATEWAY, HeaderMap::new(), Body::from("upstream down"));
///     Ok(Relay::new(status, headers, body))
/// }
/// # let _: Methods = get(models);
/// ```
pub struct Relay(Response);

impl Relay {
    /// The answer with `status`, `headers` (hop-by-hop ones removed) and `body`.
    pub fn new(status: StatusCode, mut headers: axum::http::HeaderMap, body: axum::body::Body) -> Self {
        use axum::http::header;
        let named: Vec<String> = headers
            .get_all(header::CONNECTION)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|v| v.split(','))
            .map(|name| name.trim().to_ascii_lowercase())
            .filter(|name| !name.is_empty())
            .collect();
        for name in &named {
            headers.remove(name.as_str());
        }
        for name in [
            header::CONNECTION,
            header::PROXY_AUTHENTICATE,
            header::PROXY_AUTHORIZATION,
            header::TE,
            header::TRAILER,
            header::TRANSFER_ENCODING,
            header::UPGRADE,
        ] {
            headers.remove(name);
        }
        headers.remove("keep-alive");
        let mut res = Response::new(body);
        *res.status_mut() = status;
        *res.headers_mut() = headers;
        res.extensions_mut().insert(Relayed);
        Relay(res)
    }
}

impl IntoResponse for Relay {
    fn into_response(self) -> Response {
        self.0
    }
}

impl Answer for Relay {
    const FORMAT: Format = Format::Html;
    type Inputs = AnyInputs;
}

/// On a [`Relay`]'s response: its failure status and its encoding are the upstream's, passed on
/// as they are.
#[derive(Clone, Copy)]
pub(crate) struct Relayed;

/// Fragments streamed to `<corten-live>`: what [`crate::live`] gives.
pub struct LiveResponse(pub(crate) Response);

impl IntoResponse for LiveResponse {
    fn into_response(self) -> Response {
        self.0
    }
}

impl Answer for LiveResponse {
    const FORMAT: Format = Format::Html;
    type Inputs = AnyInputs;
}

/// A feed's snapshot or live stream, read by the runtime.
#[cfg(feature = "feed")]
impl Answer for corten_feed::FeedResponse {
    const FORMAT: Format = Format::Json;
    type Inputs = AnyInputs;
}

/// An answer that is the same for everyone, so any cache (a CDN, a proxy) may keep it:
/// `async fn home(req: Req) -> Public<HtmlResult>`. Its handler may only read inputs that cannot
/// tell who is asking ([`Anonymous`]), so a `Caller`, a `HeaderMap` or a cookie there does not
/// compile:
///
/// ```compile_fail,E0277
/// use std::time::Duration;
/// use corten::{axum::http::HeaderMap, prelude::*};
///
/// #[corten::routes]
/// mod url {
///     #[route("/", get = home)]
///     pub struct Home;
/// }
///
/// async fn home(headers: HeaderMap) -> Public<&'static str> {
///     let _ = headers.get("cookie");
///     Public::new(Duration::from_secs(60), "home")
/// }
/// # fn main() { let _ = url::router(); }
/// ```
///
/// Only a successful answer is shared; a failure keeps the default, `private, no-cache`. The rules
/// of [`crate::cache`] still apply when it is sent: a request with a cookie, or a page with the
/// request's nonce, is answered privately.
pub struct Public<A: Shareable> {
    cache: Cache,
    answer: A::ForAll,
}

impl<A: Shareable> Public<A> {
    /// `answer`, which any cache may keep for `max_age` (whole seconds). Its type says the
    /// `Public`'s, so a closure needs no annotation: `Public::new(age, "home")` is a
    /// `Public<&str>`, and `Public::new(age, req.page(..))` a `Public<HtmlResult>` whose page is
    /// made for everyone.
    ///
    /// A page made for one visitor does not fit, so a helper that makes a page for a `Public`
    /// handler returns `corten::Result<HtmlResponse<For>>` for any `For`, not `HtmlResult`:
    ///
    /// ```
    /// use std::time::Duration;
    /// use corten::{prelude::*, HtmlResponse};
    ///
    /// fn home_page<For>(req: &Req) -> corten::Result<HtmlResponse<For>> {
    ///     req.page(html! { <html><head><title>"Home"</title><Corten /></head><body><main>"Home"</main></body></html> })
    /// }
    ///
    /// async fn home(req: Req) -> Public<HtmlResult> {
    ///     Public::new(Duration::from_secs(60), home_page(&req))
    /// }
    /// # let _: Methods = get(home);
    /// ```
    pub fn new<T: ForEveryone<Named = A>>(max_age: Duration, answer: T) -> Self {
        Public { cache: Cache::public(max_age), answer: answer.for_all() }
    }

    /// After `max_age`, a cache may still serve it for up to `stale` while it fetches a fresh one.
    pub fn stale_while_revalidate(mut self, stale: Duration) -> Self {
        self.cache = self.cache.stale_while_revalidate(stale);
        self
    }

    /// A shared cache (a CDN, a proxy) may keep it for `shared` (`s-maxage`, whole seconds), while
    /// the browser keeps `max_age`: a catalog page a CDN serves for a day that a browser checks
    /// again after a minute. `public, max-age=60, s-maxage=86400`. A lifetime known only when the
    /// request is answered (an API's own `s-maxage`) is passed the same way. When the answer is
    /// sent privately (a client navigation, a request with a cookie), `s-maxage` is dropped with
    /// `public`.
    pub fn shared(mut self, shared: Duration) -> Self {
        self.cache = self.cache.shared(shared);
        self
    }
}

impl<A: Shareable> IntoResponse for Public<A> {
    fn into_response(self) -> Response {
        let mut res = self.answer.into_response();
        if res.status().is_success() || res.status().is_redirection() {
            self.cache.apply(&mut res);
        }
        res
    }
}

impl<A: Answer + Shareable> Answer for Public<A> {
    const FORMAT: Format = A::FORMAT;
    type Inputs = AnonymousInputs;
}

/// An answer nothing may keep, not even the browser's own cache (`Cache-Control: no-store`): a
/// statement, a one-time code. `async fn statement(req: Req) -> NoStore<HtmlResult>`. The browser's
/// back/forward cache may not restore the page either. It applies to a failure too.
pub struct NoStore<A>(pub A);

impl<A> NoStore<A> {
    /// `answer`, which nothing may keep.
    pub fn new(answer: A) -> Self {
        NoStore(answer)
    }
}

impl<A: IntoResponse> IntoResponse for NoStore<A> {
    fn into_response(self) -> Response {
        let mut res = self.0.into_response();
        Cache::no_store().apply(&mut res);
        res
    }
}

impl<A: Answer> Answer for NoStore<A> {
    const FORMAT: Format = A::FORMAT;
    type Inputs = A::Inputs;
}

/// A handler may read anything.
pub struct AnyInputs;

/// A handler may read only [`Anonymous`] inputs.
pub struct AnonymousInputs;

/// An input that cannot tell who is asking: no cookie, no credential header, no `Caller`. A
/// [`Public`] answer's handler takes only these. Implement it for an extractor of the app's own
/// that reads nothing about the caller.
#[diagnostic::on_unimplemented(
    message = "a `Public` answer is the same for everyone, so its handler cannot take `{Self}`",
    label = "`{Self}` may tell who is asking",
    note = "return a private answer, or implement `corten::answer::Anonymous` for an extractor of the app's own that never reads who is asking"
)]
pub trait Anonymous {}

impl<T> Anonymous for axum::extract::Path<T> {}
impl<T> Anonymous for axum::extract::Query<T> {}
impl<T> Anonymous for crate::Query<T> {}
impl<T> Anonymous for crate::Form<T> {}
impl<T> Anonymous for crate::form::Upload<T> {}
impl<T> Anonymous for State<T> {}
impl<T: Anonymous> Anonymous for Option<T> {}
impl Anonymous for Req {}
impl Anonymous for Signal {}
impl Anonymous for Method {}
impl Anonymous for Uri {}
impl Anonymous for crate::LiveSince {}
#[cfg(feature = "json")]
impl<T> Anonymous for crate::Json<T> {}
#[cfg(feature = "feed")]
impl Anonymous for corten_feed::Since {}

/// Every argument is [`Anonymous`].
#[doc(hidden)]
pub trait AllAnonymous {}

/// Whether an answer's [`Answer::Inputs`] allows the handler's arguments.
#[doc(hidden)]
pub trait Accepts<Args> {}
impl<A> Accepts<A> for AnyInputs {}
impl<A: AllAnonymous> Accepts<A> for AnonymousInputs {}

/// A handler's arguments and result, by the shapes axum's `Handler` uses.
#[doc(hidden)]
pub trait Signature<T> {
    type Args;
    type Out;
}

impl<F, Fut> Signature<((),)> for F
where
    F: FnOnce() -> Fut,
    Fut: Future,
{
    type Args = ();
    type Out = Fut::Output;
}

impl AllAnonymous for () {}

macro_rules! arities {
    ($($ty:ident),*) => {
        impl<F, Fut, M, $($ty,)*> Signature<(M, $($ty,)*)> for F
        where
            F: FnOnce($($ty,)*) -> Fut,
            Fut: Future,
        {
            type Args = ($($ty,)*);
            type Out = Fut::Output;
        }

        impl<$($ty: Anonymous,)*> AllAnonymous for ($($ty,)*) {}
    };
}
arities!(A1);
arities!(A1, A2);
arities!(A1, A2, A3);
arities!(A1, A2, A3, A4);
arities!(A1, A2, A3, A4, A5);
arities!(A1, A2, A3, A4, A5, A6);
arities!(A1, A2, A3, A4, A5, A6, A7);
arities!(A1, A2, A3, A4, A5, A6, A7, A8);
arities!(A1, A2, A3, A4, A5, A6, A7, A8, A9);
arities!(A1, A2, A3, A4, A5, A6, A7, A8, A9, A10);
arities!(A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11);
arities!(A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12);
arities!(A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13);
arities!(A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14);
arities!(A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14, A15);
arities!(A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14, A15, A16);

/// The position of an argument, for [`Holds`].
#[doc(hidden)]
pub struct At<const N: usize>;

/// The handler's arguments include `T` (`I` is where, found by the compiler). A routes module
/// with `caller = Viewer` holds every handler in it to taking `Caller<Viewer>`.
#[doc(hidden)]
#[diagnostic::on_unimplemented(
    message = "every route in this module takes `{T}`, and this handler does not",
    label = "this handler",
    note = "add `{T}` to the handler's arguments, or move its route to a module without `caller = ..` (a public page)"
)]
pub trait Holds<T, I> {}

/// No requirement: `()`.
#[doc(hidden)]
pub struct Free;
impl<A> Holds<(), Free> for A {}
impl<T> Holds<T, At<0>> for (T,) {}
impl<T, A2> Holds<T, At<0>> for (T, A2) {}
impl<T, A1> Holds<T, At<1>> for (A1, T) {}
impl<T, A2, A3> Holds<T, At<0>> for (T, A2, A3) {}
impl<T, A1, A3> Holds<T, At<1>> for (A1, T, A3) {}
impl<T, A1, A2> Holds<T, At<2>> for (A1, A2, T) {}
impl<T, A2, A3, A4> Holds<T, At<0>> for (T, A2, A3, A4) {}
impl<T, A1, A3, A4> Holds<T, At<1>> for (A1, T, A3, A4) {}
impl<T, A1, A2, A4> Holds<T, At<2>> for (A1, A2, T, A4) {}
impl<T, A1, A2, A3> Holds<T, At<3>> for (A1, A2, A3, T) {}
impl<T, A2, A3, A4, A5> Holds<T, At<0>> for (T, A2, A3, A4, A5) {}
impl<T, A1, A3, A4, A5> Holds<T, At<1>> for (A1, T, A3, A4, A5) {}
impl<T, A1, A2, A4, A5> Holds<T, At<2>> for (A1, A2, T, A4, A5) {}
impl<T, A1, A2, A3, A5> Holds<T, At<3>> for (A1, A2, A3, T, A5) {}
impl<T, A1, A2, A3, A4> Holds<T, At<4>> for (A1, A2, A3, A4, T) {}
impl<T, A2, A3, A4, A5, A6> Holds<T, At<0>> for (T, A2, A3, A4, A5, A6) {}
impl<T, A1, A3, A4, A5, A6> Holds<T, At<1>> for (A1, T, A3, A4, A5, A6) {}
impl<T, A1, A2, A4, A5, A6> Holds<T, At<2>> for (A1, A2, T, A4, A5, A6) {}
impl<T, A1, A2, A3, A5, A6> Holds<T, At<3>> for (A1, A2, A3, T, A5, A6) {}
impl<T, A1, A2, A3, A4, A6> Holds<T, At<4>> for (A1, A2, A3, A4, T, A6) {}
impl<T, A1, A2, A3, A4, A5> Holds<T, At<5>> for (A1, A2, A3, A4, A5, T) {}
impl<T, A2, A3, A4, A5, A6, A7> Holds<T, At<0>> for (T, A2, A3, A4, A5, A6, A7) {}
impl<T, A1, A3, A4, A5, A6, A7> Holds<T, At<1>> for (A1, T, A3, A4, A5, A6, A7) {}
impl<T, A1, A2, A4, A5, A6, A7> Holds<T, At<2>> for (A1, A2, T, A4, A5, A6, A7) {}
impl<T, A1, A2, A3, A5, A6, A7> Holds<T, At<3>> for (A1, A2, A3, T, A5, A6, A7) {}
impl<T, A1, A2, A3, A4, A6, A7> Holds<T, At<4>> for (A1, A2, A3, A4, T, A6, A7) {}
impl<T, A1, A2, A3, A4, A5, A7> Holds<T, At<5>> for (A1, A2, A3, A4, A5, T, A7) {}
impl<T, A1, A2, A3, A4, A5, A6> Holds<T, At<6>> for (A1, A2, A3, A4, A5, A6, T) {}
impl<T, A2, A3, A4, A5, A6, A7, A8> Holds<T, At<0>> for (T, A2, A3, A4, A5, A6, A7, A8) {}
impl<T, A1, A3, A4, A5, A6, A7, A8> Holds<T, At<1>> for (A1, T, A3, A4, A5, A6, A7, A8) {}
impl<T, A1, A2, A4, A5, A6, A7, A8> Holds<T, At<2>> for (A1, A2, T, A4, A5, A6, A7, A8) {}
impl<T, A1, A2, A3, A5, A6, A7, A8> Holds<T, At<3>> for (A1, A2, A3, T, A5, A6, A7, A8) {}
impl<T, A1, A2, A3, A4, A6, A7, A8> Holds<T, At<4>> for (A1, A2, A3, A4, T, A6, A7, A8) {}
impl<T, A1, A2, A3, A4, A5, A7, A8> Holds<T, At<5>> for (A1, A2, A3, A4, A5, T, A7, A8) {}
impl<T, A1, A2, A3, A4, A5, A6, A8> Holds<T, At<6>> for (A1, A2, A3, A4, A5, A6, T, A8) {}
impl<T, A1, A2, A3, A4, A5, A6, A7> Holds<T, At<7>> for (A1, A2, A3, A4, A5, A6, A7, T) {}
impl<T, A2, A3, A4, A5, A6, A7, A8, A9> Holds<T, At<0>> for (T, A2, A3, A4, A5, A6, A7, A8, A9) {}
impl<T, A1, A3, A4, A5, A6, A7, A8, A9> Holds<T, At<1>> for (A1, T, A3, A4, A5, A6, A7, A8, A9) {}
impl<T, A1, A2, A4, A5, A6, A7, A8, A9> Holds<T, At<2>> for (A1, A2, T, A4, A5, A6, A7, A8, A9) {}
impl<T, A1, A2, A3, A5, A6, A7, A8, A9> Holds<T, At<3>> for (A1, A2, A3, T, A5, A6, A7, A8, A9) {}
impl<T, A1, A2, A3, A4, A6, A7, A8, A9> Holds<T, At<4>> for (A1, A2, A3, A4, T, A6, A7, A8, A9) {}
impl<T, A1, A2, A3, A4, A5, A7, A8, A9> Holds<T, At<5>> for (A1, A2, A3, A4, A5, T, A7, A8, A9) {}
impl<T, A1, A2, A3, A4, A5, A6, A8, A9> Holds<T, At<6>> for (A1, A2, A3, A4, A5, A6, T, A8, A9) {}
impl<T, A1, A2, A3, A4, A5, A6, A7, A9> Holds<T, At<7>> for (A1, A2, A3, A4, A5, A6, A7, T, A9) {}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8> Holds<T, At<8>> for (A1, A2, A3, A4, A5, A6, A7, A8, T) {}
impl<T, A2, A3, A4, A5, A6, A7, A8, A9, A10> Holds<T, At<0>> for (T, A2, A3, A4, A5, A6, A7, A8, A9, A10) {}
impl<T, A1, A3, A4, A5, A6, A7, A8, A9, A10> Holds<T, At<1>> for (A1, T, A3, A4, A5, A6, A7, A8, A9, A10) {}
impl<T, A1, A2, A4, A5, A6, A7, A8, A9, A10> Holds<T, At<2>> for (A1, A2, T, A4, A5, A6, A7, A8, A9, A10) {}
impl<T, A1, A2, A3, A5, A6, A7, A8, A9, A10> Holds<T, At<3>> for (A1, A2, A3, T, A5, A6, A7, A8, A9, A10) {}
impl<T, A1, A2, A3, A4, A6, A7, A8, A9, A10> Holds<T, At<4>> for (A1, A2, A3, A4, T, A6, A7, A8, A9, A10) {}
impl<T, A1, A2, A3, A4, A5, A7, A8, A9, A10> Holds<T, At<5>> for (A1, A2, A3, A4, A5, T, A7, A8, A9, A10) {}
impl<T, A1, A2, A3, A4, A5, A6, A8, A9, A10> Holds<T, At<6>> for (A1, A2, A3, A4, A5, A6, T, A8, A9, A10) {}
impl<T, A1, A2, A3, A4, A5, A6, A7, A9, A10> Holds<T, At<7>> for (A1, A2, A3, A4, A5, A6, A7, T, A9, A10) {}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A10> Holds<T, At<8>> for (A1, A2, A3, A4, A5, A6, A7, A8, T, A10) {}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A9> Holds<T, At<9>> for (A1, A2, A3, A4, A5, A6, A7, A8, A9, T) {}
impl<T, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11> Holds<T, At<0>> for (T, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11) {}
impl<T, A1, A3, A4, A5, A6, A7, A8, A9, A10, A11> Holds<T, At<1>> for (A1, T, A3, A4, A5, A6, A7, A8, A9, A10, A11) {}
impl<T, A1, A2, A4, A5, A6, A7, A8, A9, A10, A11> Holds<T, At<2>> for (A1, A2, T, A4, A5, A6, A7, A8, A9, A10, A11) {}
impl<T, A1, A2, A3, A5, A6, A7, A8, A9, A10, A11> Holds<T, At<3>> for (A1, A2, A3, T, A5, A6, A7, A8, A9, A10, A11) {}
impl<T, A1, A2, A3, A4, A6, A7, A8, A9, A10, A11> Holds<T, At<4>> for (A1, A2, A3, A4, T, A6, A7, A8, A9, A10, A11) {}
impl<T, A1, A2, A3, A4, A5, A7, A8, A9, A10, A11> Holds<T, At<5>> for (A1, A2, A3, A4, A5, T, A7, A8, A9, A10, A11) {}
impl<T, A1, A2, A3, A4, A5, A6, A8, A9, A10, A11> Holds<T, At<6>> for (A1, A2, A3, A4, A5, A6, T, A8, A9, A10, A11) {}
impl<T, A1, A2, A3, A4, A5, A6, A7, A9, A10, A11> Holds<T, At<7>> for (A1, A2, A3, A4, A5, A6, A7, T, A9, A10, A11) {}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A10, A11> Holds<T, At<8>> for (A1, A2, A3, A4, A5, A6, A7, A8, T, A10, A11) {}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A9, A11> Holds<T, At<9>> for (A1, A2, A3, A4, A5, A6, A7, A8, A9, T, A11) {}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A9, A10> Holds<T, At<10>> for (A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, T) {}
impl<T, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12> Holds<T, At<0>> for (T, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12) {}
impl<T, A1, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12> Holds<T, At<1>> for (A1, T, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12) {}
impl<T, A1, A2, A4, A5, A6, A7, A8, A9, A10, A11, A12> Holds<T, At<2>> for (A1, A2, T, A4, A5, A6, A7, A8, A9, A10, A11, A12) {}
impl<T, A1, A2, A3, A5, A6, A7, A8, A9, A10, A11, A12> Holds<T, At<3>> for (A1, A2, A3, T, A5, A6, A7, A8, A9, A10, A11, A12) {}
impl<T, A1, A2, A3, A4, A6, A7, A8, A9, A10, A11, A12> Holds<T, At<4>> for (A1, A2, A3, A4, T, A6, A7, A8, A9, A10, A11, A12) {}
impl<T, A1, A2, A3, A4, A5, A7, A8, A9, A10, A11, A12> Holds<T, At<5>> for (A1, A2, A3, A4, A5, T, A7, A8, A9, A10, A11, A12) {}
impl<T, A1, A2, A3, A4, A5, A6, A8, A9, A10, A11, A12> Holds<T, At<6>> for (A1, A2, A3, A4, A5, A6, T, A8, A9, A10, A11, A12) {}
impl<T, A1, A2, A3, A4, A5, A6, A7, A9, A10, A11, A12> Holds<T, At<7>> for (A1, A2, A3, A4, A5, A6, A7, T, A9, A10, A11, A12) {}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A10, A11, A12> Holds<T, At<8>> for (A1, A2, A3, A4, A5, A6, A7, A8, T, A10, A11, A12) {}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A9, A11, A12> Holds<T, At<9>> for (A1, A2, A3, A4, A5, A6, A7, A8, A9, T, A11, A12) {}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A12> Holds<T, At<10>> for (A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, T, A12) {}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11> Holds<T, At<11>> for (A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, T) {}
impl<T, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13> Holds<T, At<0>>
    for (T, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13)
{
}
impl<T, A1, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13> Holds<T, At<1>>
    for (A1, T, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13)
{
}
impl<T, A1, A2, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13> Holds<T, At<2>>
    for (A1, A2, T, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13)
{
}
impl<T, A1, A2, A3, A5, A6, A7, A8, A9, A10, A11, A12, A13> Holds<T, At<3>>
    for (A1, A2, A3, T, A5, A6, A7, A8, A9, A10, A11, A12, A13)
{
}
impl<T, A1, A2, A3, A4, A6, A7, A8, A9, A10, A11, A12, A13> Holds<T, At<4>>
    for (A1, A2, A3, A4, T, A6, A7, A8, A9, A10, A11, A12, A13)
{
}
impl<T, A1, A2, A3, A4, A5, A7, A8, A9, A10, A11, A12, A13> Holds<T, At<5>>
    for (A1, A2, A3, A4, A5, T, A7, A8, A9, A10, A11, A12, A13)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A8, A9, A10, A11, A12, A13> Holds<T, At<6>>
    for (A1, A2, A3, A4, A5, A6, T, A8, A9, A10, A11, A12, A13)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A7, A9, A10, A11, A12, A13> Holds<T, At<7>>
    for (A1, A2, A3, A4, A5, A6, A7, T, A9, A10, A11, A12, A13)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A10, A11, A12, A13> Holds<T, At<8>>
    for (A1, A2, A3, A4, A5, A6, A7, A8, T, A10, A11, A12, A13)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A9, A11, A12, A13> Holds<T, At<9>>
    for (A1, A2, A3, A4, A5, A6, A7, A8, A9, T, A11, A12, A13)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A12, A13> Holds<T, At<10>>
    for (A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, T, A12, A13)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A13> Holds<T, At<11>>
    for (A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, T, A13)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12> Holds<T, At<12>>
    for (A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, T)
{
}
impl<T, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14> Holds<T, At<0>>
    for (T, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14)
{
}
impl<T, A1, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14> Holds<T, At<1>>
    for (A1, T, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14)
{
}
impl<T, A1, A2, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14> Holds<T, At<2>>
    for (A1, A2, T, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14)
{
}
impl<T, A1, A2, A3, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14> Holds<T, At<3>>
    for (A1, A2, A3, T, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14)
{
}
impl<T, A1, A2, A3, A4, A6, A7, A8, A9, A10, A11, A12, A13, A14> Holds<T, At<4>>
    for (A1, A2, A3, A4, T, A6, A7, A8, A9, A10, A11, A12, A13, A14)
{
}
impl<T, A1, A2, A3, A4, A5, A7, A8, A9, A10, A11, A12, A13, A14> Holds<T, At<5>>
    for (A1, A2, A3, A4, A5, T, A7, A8, A9, A10, A11, A12, A13, A14)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A8, A9, A10, A11, A12, A13, A14> Holds<T, At<6>>
    for (A1, A2, A3, A4, A5, A6, T, A8, A9, A10, A11, A12, A13, A14)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A7, A9, A10, A11, A12, A13, A14> Holds<T, At<7>>
    for (A1, A2, A3, A4, A5, A6, A7, T, A9, A10, A11, A12, A13, A14)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A10, A11, A12, A13, A14> Holds<T, At<8>>
    for (A1, A2, A3, A4, A5, A6, A7, A8, T, A10, A11, A12, A13, A14)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A9, A11, A12, A13, A14> Holds<T, At<9>>
    for (A1, A2, A3, A4, A5, A6, A7, A8, A9, T, A11, A12, A13, A14)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A12, A13, A14> Holds<T, At<10>>
    for (A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, T, A12, A13, A14)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A13, A14> Holds<T, At<11>>
    for (A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, T, A13, A14)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A14> Holds<T, At<12>>
    for (A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, T, A14)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13> Holds<T, At<13>>
    for (A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, T)
{
}
impl<T, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14, A15> Holds<T, At<0>>
    for (T, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14, A15)
{
}
impl<T, A1, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14, A15> Holds<T, At<1>>
    for (A1, T, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14, A15)
{
}
impl<T, A1, A2, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14, A15> Holds<T, At<2>>
    for (A1, A2, T, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14, A15)
{
}
impl<T, A1, A2, A3, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14, A15> Holds<T, At<3>>
    for (A1, A2, A3, T, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14, A15)
{
}
impl<T, A1, A2, A3, A4, A6, A7, A8, A9, A10, A11, A12, A13, A14, A15> Holds<T, At<4>>
    for (A1, A2, A3, A4, T, A6, A7, A8, A9, A10, A11, A12, A13, A14, A15)
{
}
impl<T, A1, A2, A3, A4, A5, A7, A8, A9, A10, A11, A12, A13, A14, A15> Holds<T, At<5>>
    for (A1, A2, A3, A4, A5, T, A7, A8, A9, A10, A11, A12, A13, A14, A15)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A8, A9, A10, A11, A12, A13, A14, A15> Holds<T, At<6>>
    for (A1, A2, A3, A4, A5, A6, T, A8, A9, A10, A11, A12, A13, A14, A15)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A7, A9, A10, A11, A12, A13, A14, A15> Holds<T, At<7>>
    for (A1, A2, A3, A4, A5, A6, A7, T, A9, A10, A11, A12, A13, A14, A15)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A10, A11, A12, A13, A14, A15> Holds<T, At<8>>
    for (A1, A2, A3, A4, A5, A6, A7, A8, T, A10, A11, A12, A13, A14, A15)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A9, A11, A12, A13, A14, A15> Holds<T, At<9>>
    for (A1, A2, A3, A4, A5, A6, A7, A8, A9, T, A11, A12, A13, A14, A15)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A12, A13, A14, A15> Holds<T, At<10>>
    for (A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, T, A12, A13, A14, A15)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A13, A14, A15> Holds<T, At<11>>
    for (A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, T, A13, A14, A15)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A14, A15> Holds<T, At<12>>
    for (A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, T, A14, A15)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A15> Holds<T, At<13>>
    for (A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, T, A15)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14> Holds<T, At<14>>
    for (A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14, T)
{
}
impl<T, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14, A15, A16> Holds<T, At<0>>
    for (T, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14, A15, A16)
{
}
impl<T, A1, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14, A15, A16> Holds<T, At<1>>
    for (A1, T, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14, A15, A16)
{
}
impl<T, A1, A2, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14, A15, A16> Holds<T, At<2>>
    for (A1, A2, T, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14, A15, A16)
{
}
impl<T, A1, A2, A3, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14, A15, A16> Holds<T, At<3>>
    for (A1, A2, A3, T, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14, A15, A16)
{
}
impl<T, A1, A2, A3, A4, A6, A7, A8, A9, A10, A11, A12, A13, A14, A15, A16> Holds<T, At<4>>
    for (A1, A2, A3, A4, T, A6, A7, A8, A9, A10, A11, A12, A13, A14, A15, A16)
{
}
impl<T, A1, A2, A3, A4, A5, A7, A8, A9, A10, A11, A12, A13, A14, A15, A16> Holds<T, At<5>>
    for (A1, A2, A3, A4, A5, T, A7, A8, A9, A10, A11, A12, A13, A14, A15, A16)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A8, A9, A10, A11, A12, A13, A14, A15, A16> Holds<T, At<6>>
    for (A1, A2, A3, A4, A5, A6, T, A8, A9, A10, A11, A12, A13, A14, A15, A16)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A7, A9, A10, A11, A12, A13, A14, A15, A16> Holds<T, At<7>>
    for (A1, A2, A3, A4, A5, A6, A7, T, A9, A10, A11, A12, A13, A14, A15, A16)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A10, A11, A12, A13, A14, A15, A16> Holds<T, At<8>>
    for (A1, A2, A3, A4, A5, A6, A7, A8, T, A10, A11, A12, A13, A14, A15, A16)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A9, A11, A12, A13, A14, A15, A16> Holds<T, At<9>>
    for (A1, A2, A3, A4, A5, A6, A7, A8, A9, T, A11, A12, A13, A14, A15, A16)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A12, A13, A14, A15, A16> Holds<T, At<10>>
    for (A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, T, A12, A13, A14, A15, A16)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A13, A14, A15, A16> Holds<T, At<11>>
    for (A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, T, A13, A14, A15, A16)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A14, A15, A16> Holds<T, At<12>>
    for (A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, T, A14, A15, A16)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A15, A16> Holds<T, At<13>>
    for (A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, T, A15, A16)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14, A16> Holds<T, At<14>>
    for (A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14, T, A16)
{
}
impl<T, A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14, A15> Holds<T, At<15>>
    for (A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14, A15, T)
{
}

/// On a failed response from a JSON route, before or after its handler ran (a path that does not
/// parse too): the route answers JSON.
#[derive(Clone, Copy)]
pub(crate) struct Answered;

/// A handler with its answer in its type: what `get = handler` builds.
#[doc(hidden)]
pub struct Endpoint<H, O> {
    pub(crate) handler: H,
    pub(crate) answer: PhantomData<fn() -> O>,
}

impl<H: Clone, O> Clone for Endpoint<H, O> {
    fn clone(&self) -> Self {
        Endpoint { handler: self.handler.clone(), answer: PhantomData }
    }
}

/// The handler's future, marking a JSON route's failures.
#[doc(hidden)]
pub struct Marked<F, O> {
    inner: F,
    answer: PhantomData<fn() -> O>,
}

impl<F: Future<Output = Response> + Unpin, O: Answer> Future for Marked<F, O> {
    type Output = Response;

    #[inline]
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Response> {
        Pin::new(&mut self.inner).poll(cx).map(|mut res| {
            // A constant: an HTML route's check compiles away.
            if matches!(O::FORMAT, Format::Json) && (res.status().is_client_error() || res.status().is_server_error()) {
                res.extensions_mut().insert(Answered);
            }
            res
        })
    }
}

impl<H, T, S, O> Handler<T, S> for Endpoint<H, O>
where
    H: Handler<T, S>,
    H::Future: Unpin,
    O: Answer + 'static,
    S: Send + 'static,
    T: 'static,
{
    type Future = Marked<H::Future, O>;

    #[inline]
    fn call(self, req: Request, state: S) -> Self::Future {
        Marked { inner: self.handler.call(req, state), answer: PhantomData }
    }
}
