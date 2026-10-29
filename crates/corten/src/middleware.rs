//! [`Middleware`]: a step of the app's own in every request (a request id, a rate limit, a trace
//! span), at one place in Corten's order.
//!
//! The order is fixed, outside in, and each stage needs the one before it:
//!
//! 1. Compression is negotiated (`Accept-Encoding`); it is applied last.
//! 2. A request for a host the app does not answer to gets `421` and nothing else
//!    ([`crate::security::Policy::host`]); a write another site sent is refused
//!    ([`crate::Failure::CrossSite`]).
//! 3. The request gets its [`crate::Signal`], its nonce and the app.
//! 4. The deadline and the panic catch start ([`crate::Failure::TimedOut`],
//!    [`crate::Failure::Panicked`]).
//! 5. The app's middleware, in the order given ([`crate::App::middleware`]). The route has been
//!    matched, so axum's `MatchedPath` names it. Then, for a request under an area's or an API's
//!    prefix, its own middleware ([`crate::Area::middleware`], [`crate::Api::middleware`]): an
//!    outer area's before an inner one's, each in the order given. A request anywhere else never
//!    meets it.
//! 6. The handler's extractors (a [`crate::Caller`] is looked up here, only when read), the
//!    handler.
//! 7. A failure becomes its route's answer: JSON for a JSON route or a data client, the app's
//!    error page for a browser ([`crate::App::error_page`]).
//! 8. Security headers, `Cache-Control` (private when anything read a caller), compression.
//!
//! Whatever a middleware answers (a 429) leaves through 7 and 8 like a handler's answer. A header
//! it sets on a failure (a request id, `Retry-After`) stays on the error page. A middleware that
//! turns a request away answers with a [`crate::Refused`] (`Refused::too_many(wait)`,
//! `Refused::forbidden()`), answered in the request's mode as a [`crate::Caller`]'s refusal is:
//! the error page for a browser, JSON for an API or a data client, `Retry-After` on a 429.
//!
//! ```
//! use std::time::Duration;
//! use corten::{axum::{extract::Request, http::HeaderValue, response::{IntoResponse, Response}}, prelude::*};
//!
//! struct RequestId;
//!
//! impl Middleware for RequestId {
//!     async fn call(&self, req: Request, next: impl Next) -> Response {
//!         let mut res = next.run(req).await;
//!         res.headers_mut().insert("x-request-id", HeaderValue::from_static("r-1"));
//!         res
//!     }
//! }
//!
//! struct RateLimit;
//!
//! impl Middleware for RateLimit {
//!     async fn call(&self, req: Request, next: impl Next) -> Response {
//!         if req.headers().contains_key("x-flood") {
//!             return Refused::too_many(Duration::from_secs(30)).into_response();
//!         }
//!         next.run(req).await
//!     }
//! }
//!
//! let app = App::new(Routes::new()).middleware((RequestId, RateLimit));
//! # let _ = app.router();
//! ```
//!
//! Middleware that belongs to part of the app is given to that part, an area or an API, and runs
//! for each request at its prefix (its routes, nested areas and APIs, an unknown URL there),
//! never by a path compared in the middleware:
//!
//! ```
//! # use corten::{axum::{extract::Request, response::{IntoResponse, Response}}, prelude::*};
//! # struct RateLimit;
//! # impl Middleware for RateLimit {
//! #     async fn call(&self, req: Request, next: impl Next) -> Response { next.run(req).await }
//! # }
//! #[corten::routes(area = Admin)]
//! mod admin {
//!     #[route("/admin")]
//!     pub struct Admin;
//! }
//!
//! #[corten::routes(api = ApiRoot)]
//! mod api {
//!     #[route("/api")]
//!     pub struct ApiRoot;
//! }
//!
//! let app = App::new(Routes::new())
//!     .area(|area| admin::area(area).middleware(RateLimit))
//!     .api(|routes| api::api(routes).middleware(RateLimit));
//! # let _ = app.router();
//! ```
//!
//! A tower layer still goes around the whole app ([`crate::App::layer`]), outside every stage,
//! for what must see the raw request first.

use std::{convert::Infallible, future::Future, pin::Pin, sync::Arc};

use axum::{extract::Request, response::Response};
use futures_util::future::Either;

/// The rest of the request after a middleware: the next middleware, then the handler.
pub trait Next: Send {
    /// Runs the rest of the request.
    fn run(self, req: Request) -> impl Future<Output = Response> + Send;
}

/// A step of the app's own in every request, or of an area's or an API's in each request at its
/// prefix; see [`crate::middleware`]. Several are a tuple,
/// run in order: `(RequestId, RateLimit)`.
pub trait Middleware: Send + Sync + 'static {
    /// Answers the request, usually by running `next` and changing what it gives.
    fn call(&self, req: Request, next: impl Next) -> impl Future<Output = Response> + Send;
}

impl Middleware for () {
    #[inline]
    fn call(&self, req: Request, next: impl Next) -> impl Future<Output = Response> + Send {
        next.run(req)
    }
}

macro_rules! tuples {
    ($first:ident $(, $rest:ident)*) => {
        impl<$first: Middleware, $($rest: Middleware),*> Middleware for ($first, $($rest,)*) {
            #[inline]
            #[allow(non_snake_case)]
            fn call(&self, req: Request, next: impl Next) -> impl Future<Output = Response> + Send {
                let ($first, $($rest,)*) = self;
                tuples!(@chain req, next, $first $(, $rest)*)
            }
        }
    };
    (@chain $req:ident, $next:ident, $only:ident) => { $only.call($req, $next) };
    (@chain $req:ident, $next:ident, $first:ident $(, $rest:ident)+) => {
        $first.call($req, Then { middleware: ($($rest,)+), next: $next })
    };
}

tuples!(A);
tuples!(A, B);
tuples!(A, B, C);
tuples!(A, B, C, D);
tuples!(A, B, C, D, E);
tuples!(A, B, C, D, E, F);

/// The middleware after one in a tuple, then the rest.
struct Then<M, N> {
    middleware: M,
    next: N,
}

macro_rules! then {
    ($($name:ident),+) => {
        impl<'a, $($name: Middleware,)+ N: Next> Next for Then<($(&'a $name,)+), N> {
            #[inline]
            #[allow(non_snake_case)]
            fn run(self, req: Request) -> impl Future<Output = Response> + Send {
                let ($($name,)+) = self.middleware;
                <($(&'a $name,)+) as Refs>::call_refs(($($name,)+), req, self.next)
            }
        }
    };
}

/// A tuple of references to middleware, run in order.
trait Refs: Send {
    fn call_refs(self, req: Request, next: impl Next) -> impl Future<Output = Response> + Send;
}

macro_rules! refs {
    ($first:ident $(, $rest:ident)*) => {
        impl<'a, $first: Middleware, $($rest: Middleware),*> Refs for (&'a $first, $(&'a $rest,)*) {
            #[inline]
            #[allow(non_snake_case)]
            fn call_refs(self, req: Request, next: impl Next) -> impl Future<Output = Response> + Send {
                let ($first, $($rest,)*) = self;
                refs!(@chain req, next, $first $(, $rest)*)
            }
        }
    };
    (@chain $req:ident, $next:ident, $only:ident) => { $only.call($req, $next) };
    (@chain $req:ident, $next:ident, $first:ident $(, $rest:ident)+) => {
        $first.call($req, Then { middleware: ($($rest,)+), next: $next })
    };
}

/// A middleware with its type erased: an area's or an API's ([`crate::Area::middleware`],
/// [`crate::Api::middleware`]), which the request finds by its path at run time.
pub(crate) trait Erased: Send + Sync + 'static {
    fn call_boxed<'a>(&'a self, req: Request, next: Rest<'a>) -> Pin<Box<dyn Future<Output = Response> + Send + 'a>>;
}

impl<M: Middleware> Erased for M {
    fn call_boxed<'a>(&'a self, req: Request, next: Rest<'a>) -> Pin<Box<dyn Future<Output = Response> + Send + 'a>> {
        Box::pin(self.call(req, next))
    }
}

/// The middleware of some routes, in the order they run.
pub(crate) type Stack = Arc<[Arc<dyn Erased>]>;

/// The rest of the request after the app's middleware: the area's or API's middleware still to
/// run (none for most routes), then the route.
pub(crate) struct Rest<'a> {
    pub(crate) stack: &'a [Arc<dyn Erased>],
    pub(crate) route: axum::routing::Route,
}

impl Next for Rest<'_> {
    #[inline]
    fn run(self, req: Request) -> impl Future<Output = Response> + Send {
        match self.stack.split_first() {
            Some((first, stack)) => Either::Left(first.call_boxed(req, Rest { stack, route: self.route })),
            None => Either::Right(handle(self.route, req)),
        }
    }
}

/// The route alone, after the app's middleware, for a request in no area or API with middleware.
pub(crate) struct Handle(pub(crate) axum::routing::Route);

impl Next for Handle {
    #[inline]
    fn run(self, req: Request) -> impl Future<Output = Response> + Send {
        handle(self.0, req)
    }
}

/// The route's own future, not an async block around it, which would move the request once more.
#[inline]
fn handle(mut route: axum::routing::Route, req: Request) -> impl Future<Output = Response> + Send {
    futures_util::FutureExt::map(tower_service::Service::call(&mut route, req), |res: Result<Response, Infallible>| match res {
        Ok(res) => res,
        Err(never) => match never {},
    })
}

refs!(A);
refs!(A, B);
refs!(A, B, C);
refs!(A, B, C, D);
refs!(A, B, C, D, E);
then!(A);
then!(A, B);
then!(A, B, C);
then!(A, B, C, D);
then!(A, B, C, D, E);
