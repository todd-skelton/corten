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
//!    middleware, the handler, a caller type that is a rule over this one, through
//!    [`Caller::find`], and the error page) share one lookup.
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
///
/// It reads as the `T` it holds, so a handler that may have none passes it on with
/// `viewer.as_deref()`:
///
/// ```
/// use corten::{axum::http::request::Parts, prelude::*};
///
/// #[derive(Clone, Hash)]
/// struct Viewer {
///     name: String,
/// }
///
/// impl<S: Sync> Identify<S> for Viewer {
///     async fn identify(_parts: &Parts, _state: &S) -> Result<Self, Refused> {
///         Err(Refused::unauthorized())
///     }
/// }
///
/// fn greeting(viewer: Option<&Viewer>) -> String {
///     viewer.map_or("Hello".into(), |v| format!("Hello, {}", v.name))
/// }
///
/// async fn home(viewer: Option<Caller<Viewer>>) -> String {
///     greeting(viewer.as_deref())
/// }
/// ```
pub struct Caller<T>(pub T);

impl<T> std::ops::Deref for Caller<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        &self.0
    }
}

/// How a caller of this type is found: a session cookie, an API key, a token. `S` is the routes'
/// state (a session store); implement it for every `S` when the lookup needs none.
pub trait Identify<S>: Clone + Hash + Send + Sync + 'static {
    /// Who is asking, or why the request may not go on.
    fn identify(parts: &Parts, state: &S) -> impl Future<Output = Result<Self, Refused>> + Send;
}

/// Why there is no caller, from [`Identify::identify`], or why a middleware turned a request
/// away (`Refused::too_many(wait).into_response()`).
#[derive(Debug)]
pub struct Refused(Why);

#[derive(Clone, Debug)]
enum Why {
    SignIn(HeaderValue, Option<std::sync::Arc<str>>),
    /// A 401, 403 or 429, with what the client is told and, for a 429, when to try again.
    Refusal {
        status: StatusCode,
        message: Option<std::sync::Arc<str>>,
        retry_after: Option<u64>,
    },
    Unavailable(std::sync::Arc<str>),
}

impl Refused {
    /// No one is signed in, or their session ended: a browser is sent to `to` (a typed route, a
    /// [`crate::Local`] return address, or an `axum::http::Uri` on another site), the way [`crate::Req::redirect`] sends it. A data
    /// client and a JSON route get a 401.
    pub fn sign_in(to: &(impl Href + ?Sized)) -> Self {
        match crate::route::header_value(to) {
            Ok(to) => Refused(Why::SignIn(to, None)),
            Err(error) => Refused(Why::Unavailable(error.to_string().into())),
        }
    }

    /// No credentials, and nowhere to sign in (an API key that is missing or wrong): a 401.
    pub fn unauthorized() -> Self {
        Refused(Why::Refusal { status: StatusCode::UNAUTHORIZED, message: None, retry_after: None })
    }

    /// Someone is known but may not do this (not staff): a 403.
    pub fn forbidden() -> Self {
        Refused(Why::Refusal { status: StatusCode::FORBIDDEN, message: None, retry_after: None })
    }

    /// Too many failed attempts from this client (counted by the app, by address or account):
    /// a 429 with `Retry-After` (whole seconds, rounded up). A lookup that checks this before the
    /// credentials tells a locked-out client nothing about them.
    pub fn too_many(retry_after: std::time::Duration) -> Self {
        let seconds = retry_after.as_secs() + u64::from(retry_after.subsec_nanos() > 0);
        Refused(Why::Refusal { status: StatusCode::TOO_MANY_REQUESTS, message: None, retry_after: Some(seconds) })
    }

    /// What a client that reads the failure is told, in place of the status's reason: the JSON
    /// error's message, and [`crate::Api::errors`]'s: `Refused::unauthorized().message("Invalid API key")`.
    /// A browser gets the app's error page, which does not show it, and one sent to sign in is
    /// sent there as before (a data client gets the 401 with it). An [`unavailable`](Refused::unavailable)
    /// lookup's 503 keeps its status's reason: its error is logged, never sent.
    pub fn message(self, message: impl Into<std::sync::Arc<str>>) -> Self {
        let message = Some(message.into());
        Refused(match self.0 {
            Why::SignIn(to, _) => Why::SignIn(to, message),
            Why::Refusal { status, retry_after, .. } => Why::Refusal { status, message, retry_after },
            why @ Why::Unavailable(_) => why,
        })
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

/// The caller, looked up once per request and type. In an app the lookup is kept on the
/// request's [`Signal`], which its error page shares; outside one (a bare router in a test), in
/// the request's extensions, when they can be written.
async fn find<S: Sync, T: Identify<S>>(parts: &Parts, state: &S) -> Result<T, Why> {
    let signal = parts.extensions.get::<Signal>();
    let kept = match signal {
        Some(signal) => signal.found::<Found<T>>(),
        None => parts.extensions.get::<Found<T>>().cloned(),
    };
    if let Some(Found(found)) = kept {
        return found;
    }
    let found = T::identify(parts, state).await.map_err(|Refused(why)| why);
    if let Some(signal) = signal {
        // The page's key mixes in who it is for: someone, or no one of this type.
        let mut hasher = std::hash::DefaultHasher::new();
        std::any::type_name::<T>().hash(&mut hasher);
        found.as_ref().ok().hash(&mut hasher);
        signal.mark_caller(hasher.finish());
        signal.keep_found(Found(found.clone()));
    }
    found
}

impl<T> Caller<T> {
    /// Who is asking as `T`, from the request's one lookup of `T`: for a caller type that is a
    /// rule over another (a seller is a viewer who may sell), whose [`Identify`] reads the other
    /// this way rather than calling its `identify`, so the session is read once in the request,
    /// whichever types the handler, its middleware and the error page take:
    ///
    /// ```
    /// use corten::{axum::http::request::Parts, prelude::*};
    ///
    /// #[derive(Clone, Hash)]
    /// struct Viewer { name: String, sells: bool }
    ///
    /// impl<S: Sync> Identify<S> for Viewer {
    ///     async fn identify(_parts: &Parts, _state: &S) -> Result<Self, Refused> {
    ///         // A real app reads its session store here.
    ///         Ok(Viewer { name: "ada".into(), sells: true })
    ///     }
    /// }
    ///
    /// #[derive(Clone, Hash)]
    /// struct Seller(Viewer);
    ///
    /// impl<S: Sync> Identify<S> for Seller {
    ///     async fn identify(parts: &Parts, state: &S) -> Result<Self, Refused> {
    ///         let Caller(viewer) = Caller::<Viewer>::find(parts, state).await?;
    ///         if viewer.sells { Ok(Seller(viewer)) } else { Err(Refused::forbidden()) }
    ///     }
    /// }
    /// ```
    ///
    /// A refusal of `T`'s is the derived type's too, unless it maps it. Outside an
    /// [`crate::App`] (a bare router in a test) each `find` looks `T` up again.
    pub async fn find<S: Sync>(parts: &Parts, state: &S) -> Result<Self, Refused>
    where
        T: Identify<S>,
    {
        find::<S, T>(parts, state).await.map(Caller).map_err(Refused)
    }
}

/// [`find`], keeping the lookup in the request's extensions outside an app.
async fn find_mut<S: Sync, T: Identify<S>>(parts: &mut Parts, state: &S) -> Result<T, Why> {
    let found = find::<S, T>(parts, state).await;
    if parts.extensions.get::<Signal>().is_none() {
        parts.extensions.insert(Found(found.clone()));
    }
    found
}

impl<S: Send + Sync, T: Identify<S>> FromRequestParts<S> for Caller<T> {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Response> {
        find_mut::<S, T>(parts, state).await.map(Caller).map_err(|why| Refused(why).into_response())
    }
}

/// A refusal as an answer, for a [`crate::Middleware`] that turns a request away:
/// `return Refused::too_many(wait).into_response();`. It is answered in the request's mode as a
/// [`Caller`]'s refusal is: the error page for a browser (a sign-in is a redirect), JSON for an
/// API or a data client, with `Retry-After` on a 429.
impl IntoResponse for Refused {
    fn into_response(self) -> Response {
        match self.0 {
            Why::SignIn(to, message) => {
                let mut res = StatusCode::UNAUTHORIZED.into_response();
                res.extensions_mut().insert(SignIn(to));
                if let Some(message) = message {
                    res.extensions_mut().insert(crate::Failed(message));
                }
                res
            }
            Why::Refusal { status, message, retry_after } => refused(status, message, retry_after),
            Why::Unavailable(message) => unidentified(&message),
        }
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

/// A 401, 403 or 429 for a request without the caller its handler reads, with what the client is
/// told and when it may try again.
fn refused(status: StatusCode, message: Option<std::sync::Arc<str>>, retry_after: Option<u64>) -> Response {
    let mut res = status.into_response();
    res.extensions_mut().insert(crate::page::Cause::Refused);
    if let Some(message) = message {
        res.extensions_mut().insert(crate::Failed(message));
    }
    if let Some(seconds) = retry_after {
        res.headers_mut().insert(axum::http::header::RETRY_AFTER, seconds.into());
    }
    res
}

impl<S: Send + Sync, T: Identify<S>> OptionalFromRequestParts<S> for Caller<T> {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Option<Self>, Response> {
        match find_mut::<S, T>(parts, state).await {
            Ok(caller) => Ok(Some(Caller(caller))),
            Err(Why::Unavailable(_)) if parts.extensions.get::<Signal>().is_some_and(Signal::is_error_page) => Ok(None),
            Err(Why::Unavailable(message)) => Err(unidentified(&message)),
            Err(_) => Ok(None),
        }
    }
}
