//! [`Caller`]: who is asking, looked up when a handler reads it and not before.
//!
//! An app says how a caller of each kind is found with [`Identify`]: a person by their session
//! cookie, a partner by its API key. A handler that needs one takes it, and the type says what the
//! route needs:
//!
//! ```
//! use corten::{axum::http::request::Parts, prelude::*, security::cookie};
//!
//! #[derive(Clone, Hash)]
//! struct Viewer { name: String }
//!
//! impl<S: Sync> Identify<S> for Viewer {
//!     async fn identify(parts: &Parts, _state: &S) -> Result<Self, Refused> {
//!         // A real app reads its session store here.
//!         match cookie::get(&parts.headers, "viewer") {
//!             Some(name) => Ok(Viewer { name: name.to_string() }),
//!             None => Err(Refused::sign_in(&url::SignIn)),
//!         }
//!     }
//! }
//!
//! #[corten::routes]
//! mod url {
//!     #[route("/account", get = account)]
//!     pub struct Account;
//!
//!     #[route("/sign-in", get = sign_in)]
//!     pub struct SignIn;
//! }
//!
//! // Someone signed in, or the browser goes to sign in: the handler never runs without one.
//! async fn account(Caller(viewer): Caller<Viewer>) -> String {
//!     viewer.name
//! }
//!
//! async fn sign_in() -> &'static str {
//!     "sign in"
//! }
//! # let _ = App::new(url::router()).router();
//! ```
//!
//! 1. Only a handler that reads a caller pays for the lookup; a page that does not (a public
//!    landing page) never touches the session store. Several readers in one request (a
//!    middleware and the handler) share one lookup.
//! 2. Reading one makes the answer private (`Cache-Control: private`), and a [`crate::Public`]
//!    answer's handler cannot take one: that does not compile.
//! 3. The caller is part of the page's key, so after signing in or out the runtime loads the next
//!    page in full and the layout shows who is asking now.
//! 4. Without one, the request fails as [`Identify::identify`] says: a browser is sent to sign
//!    in (a 303, or `corten-location` for a form patch), a data client and a JSON route get a
//!    401, and a refusal is the app's error page for 403 ([`crate::Failure`]).
//! 5. `Option<Caller<T>>` is for a page that shows something either way (a layout's "Sign in" or
//!    "Signed in as"): `None` when there is no caller, and the handler runs.
//! 6. A routes module with `caller = Viewer` holds every handler in it to taking
//!    `Caller<Viewer>`: a route added there without it does not compile (see [`crate::routes`]).
//!
//! A [`crate::Public`] answer cannot read who is asking:
//!
//! ```compile_fail,E0277
//! use std::time::Duration;
//! use corten::{axum::http::request::Parts, prelude::*};
//!
//! #[derive(Clone, Hash)]
//! struct Partner;
//!
//! impl<S: Sync> Identify<S> for Partner {
//!     async fn identify(_parts: &Parts, _state: &S) -> Result<Self, Refused> {
//!         Err(Refused::unauthorized())
//!     }
//! }
//!
//! #[corten::routes]
//! mod url {
//!     #[route("/rates", get = rates)]
//!     pub struct Rates;
//! }
//!
//! async fn rates(_: Caller<Partner>) -> Public<&'static str> {
//!     Public::new(Duration::from_secs(60), "rates for this partner")
//! }
//! # fn main() { let _ = url::router(); }
//! ```

use std::{
    future::Future,
    hash::{Hash, Hasher},
};

use axum::{
    extract::{FromRequestParts, OptionalFromRequestParts},
    http::{request::Parts, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};

use crate::{route::Href, Error, Signal};

/// Who is asking, as `T`, found by `T`'s [`Identify`] when the handler reads it. See
/// [`crate::caller`].
pub struct Caller<T>(pub T);

/// How a caller of this type is found: a session cookie, an API key, a token. `S` is the routes'
/// state (a session store); implement it for every `S` when the lookup needs none.
pub trait Identify<S>: Clone + Hash + Send + Sync + 'static {
    /// Who is asking, or why the request may not go on.
    fn identify(parts: &Parts, state: &S) -> impl Future<Output = Result<Self, Refused>> + Send;
}

/// Why there is no caller, from [`Identify::identify`].
#[derive(Debug)]
pub struct Refused(Why);

#[derive(Clone, Debug)]
enum Why {
    SignIn(HeaderValue),
    Unauthorized,
    Forbidden,
    Unavailable(std::sync::Arc<str>),
}

impl Refused {
    /// No one is signed in, or their session ended: a browser is sent to `to` (a typed route, or
    /// an `axum::http::Uri` on another site), the way [`crate::Req::redirect`] sends it. A data
    /// client and a JSON route get a 401.
    pub fn sign_in(to: &(impl Href + ?Sized)) -> Self {
        match to.with_href(|href| HeaderValue::try_from(href)) {
            Ok(to) => Refused(Why::SignIn(to)),
            Err(error) => Refused(Why::Unavailable(error.to_string().into())),
        }
    }

    /// No credentials, and nowhere to sign in (an API key that is missing or wrong): a 401.
    pub fn unauthorized() -> Self {
        Refused(Why::Unauthorized)
    }

    /// Someone is known but may not do this (not staff): a 403.
    pub fn forbidden() -> Self {
        Refused(Why::Forbidden)
    }

    /// Who is asking cannot be known now (the session store is down): a 503, so no one is sent
    /// to sign in again for nothing. The message is logged.
    pub fn unavailable(error: impl std::fmt::Display) -> Self {
        Refused(Why::Unavailable(error.to_string().into()))
    }
}

/// The lookup's outcome for `T`, for the next reader in the request.
#[derive(Clone)]
struct Found<T>(Result<T, Why>);

/// On the response for a request sent to sign in: where to. The failure stage answers it in the
/// request's mode.
#[derive(Clone)]
pub(crate) struct SignIn(pub HeaderValue);

/// The caller, looked up once per request and type.
async fn find<S: Sync, T: Identify<S>>(parts: &mut Parts, state: &S) -> Result<T, Why> {
    if let Some(Found(found)) = parts.extensions.get::<Found<T>>() {
        return found.clone();
    }
    let found = T::identify(parts, state).await.map_err(|Refused(why)| why);
    // The page's key mixes in who it is for: someone, or no one of this type.
    let mut hasher = std::hash::DefaultHasher::new();
    std::any::type_name::<T>().hash(&mut hasher);
    found.as_ref().ok().hash(&mut hasher);
    if let Some(signal) = parts.extensions.get::<Signal>() {
        signal.mark_caller(hasher.finish());
    }
    parts.extensions.insert(Found(found.clone()));
    found
}

impl<S: Send + Sync, T: Identify<S>> FromRequestParts<S> for Caller<T> {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Response> {
        find::<S, T>(parts, state).await.map(Caller).map_err(|why| match why {
            Why::SignIn(to) => {
                let mut res = StatusCode::UNAUTHORIZED.into_response();
                res.extensions_mut().insert(SignIn(to));
                res
            }
            Why::Unauthorized => refused(StatusCode::UNAUTHORIZED),
            Why::Forbidden => refused(StatusCode::FORBIDDEN),
            Why::Unavailable(message) => unidentified(&message),
        })
    }
}

/// A 503 for a request whose caller could not be looked up: the error page's
/// [`crate::Failure::Unidentified`], not a handler's own 503.
fn unidentified(message: &str) -> Response {
    let mut res = Error::new(format!("who is asking cannot be known: {message}"))
        .with_status(StatusCode::SERVICE_UNAVAILABLE)
        .into_response();
    res.extensions_mut().insert(crate::page::Cause::Unidentified);
    res
}

/// A 401 or 403 for a request without the caller its handler reads.
fn refused(status: StatusCode) -> Response {
    let mut res = status.into_response();
    res.extensions_mut().insert(crate::page::Cause::Refused);
    res
}

impl<S: Send + Sync, T: Identify<S>> OptionalFromRequestParts<S> for Caller<T> {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Option<Self>, Response> {
        match find::<S, T>(parts, state).await {
            Ok(caller) => Ok(Some(Caller(caller))),
            Err(Why::Unavailable(_)) if parts.extensions.get::<Signal>().is_some_and(Signal::is_error_page) => Ok(None),
            Err(Why::Unavailable(message)) => Err(unidentified(&message)),
            Err(_) => Ok(None),
        }
    }
}
