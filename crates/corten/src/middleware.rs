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
//!    matched, so axum's `MatchedPath` names it.
//! 6. The handler's extractors (a [`crate::Caller`] is looked up here, only when read), the
//!    handler.
//! 7. A failure becomes its route's answer: JSON for a JSON route or a data client, the app's
//!    error page for a browser ([`crate::App::error_page`]).
//! 8. Security headers, `Cache-Control` (private when anything read a caller), compression.
//!
//! Whatever a middleware answers (a 429) leaves through 7 and 8 like a handler's answer.
//!
//! ```
//! use corten::{axum::{extract::Request, http::{HeaderValue, StatusCode}, response::{IntoResponse, Response}}, prelude::*};
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
//!             return corten::Error::status(StatusCode::TOO_MANY_REQUESTS).into_response();
//!         }
//!         next.run(req).await
//!     }
//! }
//!
//! let app = App::new(Routes::new()).middleware((RequestId, RateLimit));
//! # let _ = app.router();
//! ```
//!
//! A tower layer still goes around the whole app (`corten::serve(app.router().layer(..))`),
//! outside every stage, for what must see the raw request first.

use std::future::Future;

use axum::{extract::Request, response::Response};

/// The rest of the request after a middleware: the next middleware, then the handler.
pub trait Next: Send {
    /// Runs the rest of the request.
    fn run(self, req: Request) -> impl Future<Output = Response> + Send;
}

/// A step of the app's own in every request; see [`crate::middleware`]. Several are a tuple,
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
