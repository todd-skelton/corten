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
/// for [`HtmlResult`], [`JsonResult<T>`], [`Public`], [`NoStore`], text, a bare `StatusCode`,
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
/// [`Req::page`], [`Req::patch`] and [`Req::redirect`] give. It derefs to the response, for a
/// header of the handler's own (`res.headers_mut().append(SET_COOKIE, ..)`).
pub struct HtmlResponse(pub(crate) Response);

impl IntoResponse for HtmlResponse {
    fn into_response(self) -> Response {
        self.0
    }
}

impl Deref for HtmlResponse {
    type Target = Response;

    fn deref(&self) -> &Response {
        &self.0
    }
}

impl DerefMut for HtmlResponse {
    fn deref_mut(&mut self) -> &mut Response {
        &mut self.0
    }
}

impl Answer for HtmlResponse {
    const FORMAT: Format = Format::Html;
    type Inputs = AnyInputs;
}

/// What a page handler returns: `async fn item(req: Req) -> HtmlResult`. A failure is the app's
/// error page for a browser.
pub type HtmlResult = crate::Result<HtmlResponse>;

/// What a JSON handler returns: `async fn price(..) -> JsonResult<Quote>`. Every failure of the
/// route, a path or body that does not parse included, is JSON. Its error is a [`JsonError`], so
/// the handler `?`s its input's field errors (a 422 with the fields) next to any other error.
#[cfg(feature = "json")]
pub type JsonResult<T> = std::result::Result<crate::Json<T>, JsonError>;

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
/// was written for (its scripts' nonces and hosts). Without one, the app's applies. When the client leaves, the body is
/// dropped, and with it the upstream request. What else to drop (`Set-Cookie`, say) is the
/// route's to decide before it builds the answer.
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

/// On a [`Relay`]'s response: its failure status is the upstream's, passed on as it is.
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
pub struct Public<A> {
    cache: Cache,
    answer: A,
}

impl<A> Public<A> {
    /// `answer`, which any cache may keep for `max_age` (whole seconds).
    pub fn new(max_age: Duration, answer: A) -> Self {
        Public { cache: Cache::public(max_age), answer }
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

impl<A: IntoResponse> IntoResponse for Public<A> {
    fn into_response(self) -> Response {
        let mut res = self.answer.into_response();
        if res.status().is_success() || res.status().is_redirection() {
            self.cache.apply(&mut res);
        }
        res
    }
}

impl<A: Answer> Answer for Public<A> {
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
