//! Typed routes: a struct per URL, so a link cannot point at a route that does not exist or miss
//! one of its parameters.
//!
//! ```ignore
//! #[derive(Route)]
//! #[route("/items/{id}")]
//! pub struct ItemRoute<'a> { pub id: &'a str, pub tab: Option<&'a str> }
//!
//! Router::new().route(ItemRoute::PATH, get(show));               // the same path axum matches
//! html! { <a href={ItemRoute { id: &item.id, tab: None }}>{item.title}</a> }
//! req.redirect(&ItemRoute { id, tab: Some("offers") }.href())    // "/items/x?tab=offers"
//! ```
//!
//! Fields named in the path fill it, percent-encoded (`{*rest}` keeps its slashes); every other
//! field is a query parameter, left out when it is `None`. Field types only need `Display`, so a
//! link borrows (`&str`) and copies nothing. The derive checks the path when it compiles: a
//! parameter without a field, a parameter that is not a whole segment, or a character that would
//! need escaping is an error. Renaming the route's path changes every link with it.
//!
//! Handlers read parameters with axum's `Path` and `Query` as before; a route struct with owned
//! fields that also derives `Deserialize` can be the extractor's type. A route struct says where a
//! link goes; that a handler is registered at `PATH` is the router's job (and its tests').
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
//! let _router: Router = Router::new().route(ItemRoute::PATH, get(|| async { "item" }));
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
//! Nor does a link that leaves a parameter out:
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
}

/// Something that names a URL: a path string or a typed route. Used by
/// [`crate::Document::current`].
pub trait Href {
    /// Calls `f` with the URL; a string is passed through without copying.
    fn with_href<T>(&self, f: impl FnOnce(&str) -> T) -> T;
}

impl Href for str {
    fn with_href<T>(&self, f: impl FnOnce(&str) -> T) -> T {
        f(self)
    }
}

impl Href for String {
    fn with_href<T>(&self, f: impl FnOnce(&str) -> T) -> T {
        f(self)
    }
}

impl<R: Route> Href for R {
    fn with_href<T>(&self, f: impl FnOnce(&str) -> T) -> T {
        f(&self.href())
    }
}

/// Bytes written as they are in a path segment or query value; everything else is %XX.
fn unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~')
}

struct Encode<'a> {
    out: &'a mut String,
    keep_slash: bool,
}

impl fmt::Write for Encode<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        let bytes = s.as_bytes();
        let mut start = 0;
        for (i, &b) in bytes.iter().enumerate() {
            if unreserved(b) || (self.keep_slash && b == b'/') {
                continue;
            }
            self.out.push_str(&s[start..i]);
            self.out.push('%');
            self.out.push(HEX[(b >> 4) as usize] as char);
            self.out.push(HEX[(b & 15) as usize] as char);
            start = i + 1;
        }
        self.out.push_str(&s[start..]);
        Ok(())
    }
}

/// Used by the code `#[derive(Route)]` generates. Not a public API.
#[doc(hidden)]
pub mod __private {
    use std::fmt::{self, Display, Write as _};

    use super::Encode;

    /// One path segment, percent-encoded (a `/` in the value is `%2F`).
    #[inline]
    pub fn segment(value: &(impl Display + ?Sized), out: &mut String) {
        let _ = write!(Encode { out, keep_slash: false }, "{value}");
    }

    /// The rest of the path (`{*rest}`): encoded, slashes kept.
    pub fn rest(value: &(impl Display + ?Sized), out: &mut String) {
        let _ = write!(Encode { out, keep_slash: true }, "{value}");
    }

    fn pair(key: &str, value: &dyn Display, first: &mut bool, html: bool, out: &mut String) {
        out.push_str(match (*first, html) {
            (true, _) => "?",
            (false, true) => "&amp;",
            (false, false) => "&",
        });
        *first = false;
        out.push_str(key);
        let _ = fmt::write(&mut Encode { out, keep_slash: false }, format_args!("{value}"));
    }

    // `Option` fields are left out when `None`; any other `Display` field is always written.
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
}
