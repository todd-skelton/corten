//! HTTP caching of what the app renders.
//!
//! Static files and the runtime are content-addressed and cached forever ([`crate::assets`]).
//! Every other response [`crate::App`] serves that does not say otherwise gets
//! `Cache-Control: private, no-cache`: the browser may keep it but asks the server before reusing
//! it, and no shared cache (a CDN, a proxy) stores it, since a page carries the shell built from
//! [`crate::App::context`] and may carry the signed-in user's data.
//!
//! A response the same for everyone opts in with [`Cache::public`], returned next to it:
//!
//! ```ignore
//! async fn home(req: Req) -> corten::Result<(Cache, Response)> {
//!     Ok((Cache::public(Duration::from_secs(60)), req.page("Home", view)?))
//! }
//! ```
//!
//! `App` lets shared caches keep it only when nothing about the request could make it differ
//! between users:
//!
//! - when the request carried credentials (`Cookie` or `Authorization`), or the context function
//!   returned a value for it, the response is `private, no-cache`;
//! - when it is not the full document (a client navigation, a form answer, a data client's JSON
//!   from the same URL as the page), only the browser keeps it (`private`, same lifetime), so a
//!   cache that ignores `Vary` cannot hand one form to another client. An [`crate::App::api`]
//!   route has one form only, so it may be shared;
//! - a response that keeps a lifetime also varies on `cookie`: after the user signs in or out,
//!   the browser fetches the page again instead of reusing the other one.
//!
//! Two things are never shared or kept, whoever set the header (a `Cache` or the handler's own
//! `Cache-Control`): a response that sets a cookie, and HTML with this request's CSP nonce
//! (`doc.nonce()` in the shell, or a handler that takes [`crate::security::Nonce`]). They get
//! `private, no-cache`, and `corten dev` logs why.
//!
//! What Corten checks is what it can see. A handler that reads a cookie through its own extractor
//! is personal too: do not mark it public. A `Cache-Control` the handler writes itself is the
//! app's own choice apart from the two rules above (a file route with `public, max-age=...` stays
//! shared). A cache in front must key on `Vary` (`corten`, `accept`, `accept-encoding`, and
//! `accept-language` with [`crate::App::messages`]).

use std::time::Duration;

use axum::{
    http::{header, HeaderMap, HeaderValue},
    response::{IntoResponseParts, Response, ResponseParts},
};

/// What caches may do with a response; see [`crate::cache`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cache {
    kind: Kind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Public { max_age: u64, stale: u64 },
    NoStore,
}

impl Cache {
    /// Any cache may keep the response for `max_age` (whole seconds) and give it to anyone, when
    /// nothing about the request makes it personal.
    pub fn public(max_age: Duration) -> Self {
        Cache { kind: Kind::Public { max_age: max_age.as_secs(), stale: 0 } }
    }

    /// After `max_age`, a cache may still serve the response for up to `stale` while it fetches
    /// a fresh one in the background.
    pub fn stale_while_revalidate(self, stale: Duration) -> Self {
        match self.kind {
            Kind::Public { max_age, .. } => Cache { kind: Kind::Public { max_age, stale: stale.as_secs() } },
            Kind::NoStore => self,
        }
    }

    /// Nothing keeps the response, not even the browser's own cache (a statement, a one-time
    /// code). The browser's back/forward cache may not restore such a page either.
    pub fn no_store() -> Self {
        Cache { kind: Kind::NoStore }
    }

    fn header(self) -> HeaderValue {
        match self.kind {
            Kind::Public { max_age, stale: 0 } => format!("public, max-age={max_age}"),
            Kind::Public { max_age, stale } => format!("public, max-age={max_age}, stale-while-revalidate={stale}"),
            Kind::NoStore => return HeaderValue::from_static("no-store"),
        }
        .try_into()
        .expect("digits are a valid header")
    }
}

impl IntoResponseParts for Cache {
    type Error = std::convert::Infallible;

    fn into_response_parts(self, mut res: ResponseParts) -> Result<ResponseParts, Self::Error> {
        res.headers_mut().insert(header::CACHE_CONTROL, self.header());
        res.extensions_mut().insert(Typed);
        Ok(res)
    }
}

/// Marks a `Cache-Control` that came from [`Cache`], which [`settle`] checks against the request.
#[derive(Clone, Copy)]
struct Typed;

/// The default for what the app renders.
pub(crate) const PRIVATE: HeaderValue = HeaderValue::from_static("private, no-cache");

/// What the layer knows about a request, for [`settle`].
pub(crate) struct Seen {
    /// A `Cookie` or `Authorization` header, or a value from the context function.
    pub personal: bool,
    /// Not the full document of a page route (a navigation, a form answer, JSON for a data client).
    pub partial: bool,
    /// The response's HTML has this request's nonce.
    pub nonce: bool,
    pub dev: bool,
}

/// True when the request carries credentials.
pub(crate) fn credentials(headers: &HeaderMap) -> bool {
    headers.contains_key(header::COOKIE) || headers.contains_key(header::AUTHORIZATION)
}

/// Sets the response's `Cache-Control`: the default when it has none, and a private one instead
/// of a shared one when the response may differ between users.
pub(crate) fn settle(res: &mut Response, seen: Seen, path: &str) {
    // One lookup for the usual response, which says nothing.
    let text = match res.headers_mut().entry(header::CACHE_CONTROL) {
        header::Entry::Vacant(entry) => {
            entry.insert(PRIVATE);
            return;
        }
        header::Entry::Occupied(entry) => entry.get().to_str().unwrap_or("private").to_string(),
    };
    let typed = res.extensions().get::<Typed>().is_some();
    let headers = res.headers_mut();
    if has(&text, "private") || has(&text, "no-store") {
        return;
    }
    let bug = if seen.nonce {
        Some("its HTML has this request's CSP nonce (doc.nonce() in the shell, or a handler that takes Nonce)")
    } else if headers.contains_key(header::SET_COOKIE) {
        Some("it sets a cookie")
    } else {
        None
    };
    if let Some(bug) = bug {
        if seen.dev {
            tracing::warn!("corten: {path} is marked for shared caches, but {bug}, so it was sent private, no-cache");
        }
        headers.insert(header::CACHE_CONTROL, PRIVATE);
        return;
    }
    if !typed {
        return;
    }
    if seen.personal {
        headers.insert(header::CACHE_CONTROL, PRIVATE);
        return;
    }
    if seen.partial {
        headers.insert(header::CACHE_CONTROL, private(&text));
    }
    headers.append(header::VARY, HeaderValue::from_static("cookie"));
}

/// True when `Cache-Control` text has `directive`.
fn has(text: &str, directive: &str) -> bool {
    text.split(',').any(|d| d.split('=').next().unwrap_or("").trim().eq_ignore_ascii_case(directive))
}

/// The same lifetime for the browser alone: `public` and `s-maxage` become `private`.
fn private(text: &str) -> HeaderValue {
    let mut out = String::from("private");
    for directive in text.split(',').map(str::trim).filter(|d| !d.is_empty()) {
        let name = directive.split('=').next().unwrap_or("").trim();
        if !name.eq_ignore_ascii_case("public") && !name.eq_ignore_ascii_case("s-maxage") {
            out.push_str(", ");
            out.push_str(directive);
        }
    }
    HeaderValue::try_from(out).unwrap_or(PRIVATE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;

    fn seen() -> Seen {
        Seen { personal: false, partial: false, nonce: false, dev: false }
    }

    /// The `Cache-Control` sent for a response with `cache` (from a [`Cache`] when `typed`).
    fn settled(cache: Option<&'static str>, typed: bool, seen: Seen, cookie: bool) -> String {
        let mut res = "x".into_response();
        if let Some(cache) = cache {
            res.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
        }
        if typed {
            res.extensions_mut().insert(Typed);
        }
        if cookie {
            res.headers_mut().insert(header::SET_COOKIE, HeaderValue::from_static("a=b"));
        }
        settle(&mut res, seen, "/x");
        res.headers()[header::CACHE_CONTROL].to_str().unwrap().to_string()
    }

    #[test]
    fn the_header_says_what_was_asked() {
        let minute = Duration::from_secs(60);
        assert_eq!(Cache::public(minute).header(), "public, max-age=60");
        assert_eq!(
            Cache::public(minute).stale_while_revalidate(Duration::from_secs(600)).header(),
            "public, max-age=60, stale-while-revalidate=600"
        );
        assert_eq!(Cache::public(Duration::from_millis(1500)).header(), "public, max-age=1", "whole seconds");
        assert_eq!(Cache::no_store().stale_while_revalidate(minute).header(), "no-store");
    }

    #[test]
    fn a_response_is_shared_only_when_nothing_makes_it_personal() {
        const PUBLIC: &str = "public, max-age=60, s-maxage=600, stale-while-revalidate=30";
        let public = Some(PUBLIC);
        assert_eq!(settled(None, false, seen(), false), "private, no-cache", "the default");
        assert_eq!(settled(public, true, seen(), false), PUBLIC);
        let partial = Seen { partial: true, ..seen() };
        assert_eq!(settled(public, true, partial, false), "private, max-age=60, stale-while-revalidate=30");
        assert_eq!(settled(Some("max-age=60"), true, Seen { partial: true, ..seen() }, false), "private, max-age=60");
        assert_eq!(settled(public, true, Seen { personal: true, ..seen() }, false), "private, no-cache");
        for personal in [Seen { personal: true, ..seen() }, Seen { partial: true, ..seen() }] {
            assert_eq!(settled(public, false, personal, false), PUBLIC, "the handler's own header is its choice");
        }
        for typed in [true, false] {
            assert_eq!(settled(public, typed, Seen { nonce: true, ..seen() }, false), "private, no-cache");
            assert_eq!(settled(public, typed, Seen { nonce: true, dev: true, ..seen() }, false), "private, no-cache");
            assert_eq!(settled(public, typed, seen(), true), "private, no-cache", "it sets a cookie");
        }
        assert_eq!(settled(Some("Private, max-age=60"), true, Seen { nonce: true, ..seen() }, true), "Private, max-age=60");
        assert_eq!(settled(Some("no-store"), true, Seen { personal: true, ..seen() }, true), "no-store");
    }

    #[test]
    fn a_response_that_keeps_a_lifetime_varies_on_the_cookie() {
        let vary = |seen: Seen| {
            let mut res = (Cache::public(Duration::from_secs(60)), "x").into_response();
            settle(&mut res, seen, "/x");
            res.headers().get_all(header::VARY).iter().any(|v| v == "cookie")
        };
        assert!(vary(seen()) && vary(Seen { partial: true, ..seen() }));
        assert!(!vary(Seen { personal: true, ..seen() }), "no-cache: nothing to reuse");
    }
}
