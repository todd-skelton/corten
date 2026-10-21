//! Typed messages: catalogs checked and compiled when the app builds.
//!
//! ```ignore
//! corten::i18n::messages! {
//!     pub struct Msg {
//!         "en" => "locales/en.json",      // the first is the default and the reference
//!         "fr" => "locales/fr.json",
//!     }
//! }
//!
//! let t: Msg = req.t();                    // negotiated from Accept-Language
//! html! { <h2>{t.item_listings()}</h2><span>{t.item_listing_count(summary.count)}</span> }
//! ```
//!
//! Catalogs are JSON objects of strings; nested objects make dotted keys, and each key is a method
//! (`item.listing_count` is `item_listing_count`). `{name}` in a message is an argument, so
//! `"{count} listings"` takes one parameter, `count`, of any `Display` type; `{{` is a brace.
//!
//! What fails the build: a key one catalog has and another lacks, a message whose arguments
//! differ between locales, a misspelled key or a missing argument at the call site, a brace that
//! is not an argument, and a value that is not a string. Changing a catalog file rebuilds the
//! crate. The messages are split into text and arguments at compile time: a message without
//! arguments is a `&'static str`, and one with arguments writes its pieces straight into the page,
//! escaped, with no lookup and no intermediate string.
//!
//! Numbers, money and dates are formatted by the app before they are passed in, with the
//! negotiated locale (`t.locale()` is a BCP 47 tag for a CLDR library such as ICU4X). Plural
//! rules are not built in: use a key per form.
//!
//! ```
//! corten::i18n::messages! {
//!     struct Msg { "en" => "src/i18n/en.json", "fr" => "src/i18n/fr.json" }
//! }
//! use corten::i18n::Messages as _;
//!
//! let t = Msg::negotiate(Some("fr"));
//! assert_eq!(t.item_listing_count(3).to_string(), "3 annonces");
//! ```
//!
//! A catalog without one of the default's keys does not compile:
//!
//! ```compile_fail
//! corten::i18n::messages! {
//!     struct Msg { "en" => "src/i18n/en.json", "de" => "src/i18n/missing-key.json" }
//! }
//! ```
//!
//! Nor one whose message takes different arguments:
//!
//! ```compile_fail
//! corten::i18n::messages! {
//!     struct Msg { "en" => "src/i18n/en.json", "de" => "src/i18n/other-args.json" }
//! }
//! ```
//!
//! Nor a call that leaves out an argument, or a key that does not exist:
//!
//! ```compile_fail
//! corten::i18n::messages! { struct Msg { "en" => "src/i18n/en.json" } }
//! let text = Msg::default().item_listing_count().to_string();
//! ```
//!
//! ```compile_fail
//! corten::i18n::messages! { struct Msg { "en" => "src/i18n/en.json" } }
//! let text = Msg::default().item_listing();
//! ```

use std::fmt::{self, Display};

use crate::view::{escape_display, AttrValue, Render};

/// Compiles catalogs into a messages type. See the [module docs](self).
pub use corten_macros::messages;

/// A type from [`messages!`]: one locale of the app's catalogs.
pub trait Messages: Copy + 'static {
    /// The locales, default first.
    const LOCALES: &'static [&'static str];
    /// The locale at `index` in [`Messages::LOCALES`], or the default when out of range.
    fn from_index(index: usize) -> Self;
    fn index(self) -> usize;

    /// This locale's tag, as listed in `messages!`.
    fn locale(self) -> &'static str {
        Self::LOCALES[self.index()]
    }

    /// The best locale for an `Accept-Language` header.
    fn negotiate(accept_language: Option<&str>) -> Self {
        Self::from_index(negotiate(Self::LOCALES, accept_language))
    }

    /// The locale with this tag (case-insensitive), or the default.
    fn for_locale(tag: &str) -> Self {
        Self::from_index(Self::LOCALES.iter().position(|l| l.eq_ignore_ascii_case(tag)).unwrap_or(0))
    }
}

/// The index in `locales` that best matches an `Accept-Language` header: highest `q` first, an
/// exact tag before the primary language (`fr-CA` matches `fr`), then 0. Allocates nothing.
pub fn negotiate(locales: &[&str], accept_language: Option<&str>) -> usize {
    let mut best: Option<(f32, usize)> = None;
    for part in accept_language.unwrap_or("").split(',') {
        let mut pieces = part.trim().split(';');
        let tag = pieces.next().unwrap_or("").trim();
        let q = match pieces.find_map(|p| p.trim().strip_prefix("q=")) {
            Some(q) => match q.parse::<f32>() {
                Ok(q) => q,
                Err(_) => continue,
            },
            None => 1.0,
        };
        if tag.is_empty() || tag == "*" || q <= 0.0 || best.is_some_and(|(b, _)| q <= b) {
            continue;
        }
        let primary = tag.split('-').next().unwrap_or(tag);
        let found = locales
            .iter()
            .position(|l| l.eq_ignore_ascii_case(tag))
            .or_else(|| locales.iter().position(|l| l.split('-').next().unwrap_or("").eq_ignore_ascii_case(primary)));
        if let Some(index) = found {
            best = Some((q, index));
        }
    }
    best.map_or(0, |(_, index)| index)
}

/// Where a message writes: the page (escaped) or plain text.
pub struct Sink<'a> {
    out: &'a mut String,
    html: bool,
}

impl Sink<'_> {
    /// A run of the message's own text, given plain and already escaped.
    #[inline]
    pub fn text(&mut self, plain: &str, html: &str) {
        self.out.push_str(if self.html { html } else { plain });
    }

    /// An argument: escaped in a page, as is in plain text.
    #[inline]
    pub fn arg<T: Display + ?Sized>(&mut self, value: &T) {
        if self.html {
            escape_display(value, self.out);
        } else {
            let _ = fmt::Write::write_fmt(self.out, format_args!("{value}"));
        }
    }
}

/// A message with its arguments, from a [`messages!`] method. In `html!` it renders escaped (as
/// text or an attribute value); `to_string()` gives the plain text.
pub struct Text<F>(F);

impl<F: Fn(&mut Sink<'_>)> Text<F> {
    #[doc(hidden)]
    pub fn new(write: F) -> Self {
        Text(write)
    }
}

impl<F: Fn(&mut Sink<'_>)> Render for Text<F> {
    #[inline]
    fn render(&self, out: &mut String) {
        (self.0)(&mut Sink { out, html: true })
    }
}

impl<F: Fn(&mut Sink<'_>)> AttrValue for Text<F> {
    fn write_attr(&self, name: &str, out: &mut String) {
        out.push_str(name);
        out.push_str("=\"");
        self.render(out);
        out.push('"');
    }
}

impl<F: Fn(&mut Sink<'_>)> Display for Text<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        (self.0)(&mut Sink { out: &mut out, html: false });
        f.write_str(&out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    crate::i18n::messages! {
        /// The test catalogs.
        struct Msg {
            "en" => "src/i18n/en.json",
            "fr" => "src/i18n/fr.json",
        }
    }

    #[test]
    fn messages_are_methods_with_typed_arguments() {
        let en = Msg::negotiate(Some("en-US"));
        let fr = Msg::negotiate(Some("fr-CA, en;q=0.5"));
        assert_eq!((en.locale(), fr.locale()), ("en", "fr"));
        assert_eq!(en.item_listings(), "Listings");
        assert_eq!(fr.item_listings(), "Annonces");
        assert_eq!(en.item_listing_count(3).to_string(), "3 listings");
        assert_eq!(fr.item_image("<Pika>", 2).to_string(), "Image 2 de <Pika>", "arguments may be in another order");
        assert_eq!(en.item_image("<Pika>", 2).to_html(), "&lt;Pika&gt; image 2", "escaped in a page");
        assert_eq!(en.brace().to_html(), "{literal} &amp; more", "the message's own text is escaped too");
        let view = crate::html! { <img alt={fr.item_image("A & B", 1)}><p>{en.item_listing_count(1)}</p> };
        assert_eq!(view.to_html(), r#"<img alt="Image 1 de A &amp; B"><p>1 listings</p>"#);
        assert_eq!(Msg::for_locale("FR"), fr);
        assert_eq!(Msg::default().locale(), "en");
    }

    #[test]
    fn negotiation_prefers_quality_then_primary_language() {
        let locales = ["en", "fr", "pt-BR"];
        let pick = |header| locales[negotiate(&locales, header)];
        assert_eq!(pick(Some("fr-CA,fr;q=0.9,en;q=0.8")), "fr");
        assert_eq!(pick(Some("de, pt-br;q=0.5, en;q=0.4")), "pt-BR");
        assert_eq!(pick(Some("en;q=0.2, fr;q=0.9")), "fr");
        assert_eq!(pick(Some("de")), "en");
        assert_eq!(pick(Some("fr;q=0")), "en", "q=0 means not acceptable");
        assert_eq!(pick(Some("fr;q=x, pt")), "pt-BR", "a bad q skips that tag only");
        assert_eq!(pick(None), "en");
    }
}
