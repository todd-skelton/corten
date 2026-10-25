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
//! `"{count} listings"` takes one parameter, `count`, of any `Display` type; `{{` is a brace. An
//! argument may be named like a Rust keyword (`{type}` is the parameter `r#type`) or in camel
//! case (`{updatedAt}` is the parameter `updated_at`), so a catalog shared with other apps
//! compiles as it is; `by_key` and `Message<"updatedAt">` use the catalog's names.
//!
//! What fails the build: a key one catalog has and another lacks, a message whose arguments
//! differ between locales, a misspelled key or a missing argument at the call site, a brace that
//! is not an argument, and a value that is not a string. Changing a catalog file rebuilds the
//! crate. The messages are split into text and arguments at compile time: a message without
//! arguments is a `&'static str`, and one with arguments writes its pieces straight into the page,
//! escaped, with no lookup and no intermediate string.
//!
//! # A message chosen at run time
//!
//! An API that answers with a message key and its arguments, for the client to show in the user's
//! locale, names a message the page cannot name in its source. `t.by_key(key, &args)` finds it by
//! its catalog key (the dotted key as written in the JSON) and fills its arguments by name from
//! `args` (an [`Args`]: pairs, a map, or a JSON object of the answer). It is `None` when the
//! catalog has no such key or `args` lacks one of the message's arguments, so the page decides what
//! to show instead, and it renders escaped like any other message:
//!
//! ```
//! corten::i18n::messages! { struct Msg { "en" => "src/i18n/en.json", "fr" => "src/i18n/fr.json" } }
//! use corten::i18n::Messages as _;
//!
//! let fr = Msg::for_locale("fr");
//! let found = fr.by_key("item.image", &[("title", "<Pika>"), ("n", "2")]).unwrap();
//! assert_eq!(found.to_string(), "Image 2 de <Pika>");
//! assert!(fr.by_key("item.missing", &[("n", 1)]).is_none(), "no such key");
//! assert!(fr.by_key("item.image", &[("n", 1)]).is_none(), "no title");
//! ```
//!
//! Plural rules are not built in: use a key per form.
//!
//! # Numbers, money and dates
//!
//! `t` writes them in its locale too, on the server, so a page shows a price or a time in the
//! reader's form with no script and nothing that changes after it loads:
//!
//! ```
//! corten::i18n::messages! { struct Msg { "en" => "src/i18n/en.json", "fr" => "src/i18n/fr.json" } }
//! use corten::i18n::{Currency, DateStyle, Decimal, Messages as _, Minor, Offset, TimeStyle};
//! const USD: Currency = Currency::new("USD");
//!
//! let (en, fr) = (Msg::for_locale("en"), Msg::for_locale("fr"));
//! assert_eq!(en.number(1234567).to_string(), "1,234,567");
//! assert_eq!(en.money(Minor(123450), USD).to_string(), "$1,234.50");
//! assert_eq!(fr.money(Minor(123450), USD).to_string(), "1\u{202f}234,50\u{a0}$US");
//! // An amount an API sends as text: never a float.
//! assert_eq!(en.money(Decimal::parse("21.50").unwrap(), Currency::new("EUR")).to_string(), "€21.50");
//!
//! let at = corten::i18n::parse_timestamp("2026-03-22T12:00:00Z").unwrap(); // or any Into<SystemTime>
//! assert_eq!(en.date_time(at, DateStyle::Medium, TimeStyle::Short).to_string(), "Mar 22, 2026, 12:00 PM UTC");
//! assert_eq!(fr.date(at, DateStyle::Long).to_string(), "22 mars 2026");
//! assert_eq!(en.time(at, TimeStyle::Short).zone(Offset::hours(-5)).to_string(), "7:00 AM GMT-5");
//! // A message argument like any other ("{count} listings").
//! assert_eq!(en.item_listing_count(en.number(1234)).to_string(), "1,234 listings");
//! ```
//!
//! Each renders escaped in `html!`, as text or an attribute value. The data (separators, currency
//! symbols, date patterns, month names) is CLDR's, as Intl in Node writes it, checked against Intl
//! for every locale (`scripts/locale-data.mjs`). `messages!` puts in the app only the locales it
//! lists, and it is linked only when the app formats something (measured: `examples/hello`, `en`,
//! grew 22.9 KB with the code; item-page, `en` and `fr`, not at all). A tag without data of its
//! own has its language's (`fr-LU` is `fr`), and a language Corten has no data for is a build
//! error. Numbers are written in the locale's digits (`fa` has `۱٬۲۳۴`), dates in the Gregorian
//! calendar.
//!
//! - `t.number(n)`: an integer or a [`Decimal`], with the fraction digits it has.
//! - `t.money(amount, currency)`: [`Minor`] units (`Minor(123450)` cents) or a [`Decimal`] in the
//!   major unit, with the digits the [`Currency`] is shown with (2, 0 for JPY, 3 for KWD), rounded
//!   half away from zero. A bare integer does not compile, since it could be either:
//!
//! ```compile_fail
//! corten::i18n::messages! { struct Msg { "en" => "src/i18n/en.json" } }
//! let price = Msg::default().money(123450, corten::i18n::Currency::new("USD"));
//! ```
//!
//! - `t.date(at, DateStyle)`, `t.time(at, TimeStyle)` and `t.date_time(at, DateStyle,
//!   TimeStyle)`: Intl's date styles (`Short` is `3/22/2026`, `Medium` `Mar 22, 2026`, `Long`
//!   `March 22, 2026`, `Full` with the weekday) and a time with its zone (`3:04 PM UTC`, `Medium`
//!   with seconds). `at` is a `SystemTime` or anything that converts to one (`chrono`'s, `jiff`'s
//!   and `time`'s timestamps do), and [`parse_timestamp`] reads RFC 3339 text. The zone is UTC,
//!   so a page reads the same wherever it is rendered; `.zone(z)` takes a [`Zone`]: an
//!   [`Offset`], or a zone with daylight saving time (from `chrono-tz` or `jiff`) that gives its
//!   offset at the instant. Another zone is written as its offset (`GMT-5`).
//!
//! Nor does a locale with no data, or its language's:
//!
//! ```compile_fail
//! corten::i18n::messages! { struct Msg { "en" => "src/i18n/en.json", "tlh" => "src/i18n/en.json" } }
//! ```
//!
//! A currency is a valid code, checked when the build evaluates the `const`:
//!
//! ```compile_fail
//! const USD: corten::i18n::Currency = corten::i18n::Currency::new("US");
//! ```
//!
//! # Text a script fills in
//!
//! Text that depends on what a script computed ("3 selected") is a message too. `t.js()` has
//! the messages with arguments unfilled, and a handler takes one as a function: the markup passes
//! `js::count(total, t.js().rows_selected())`, the script's JSDoc says
//! `@param {Message<"count">} label` (`Message` from `"corten"`) and it shows
//! `label({ count: 3 })` with `textContent`. Only the messages a page passes are sent, in the
//! request's locale, as their text and argument names, in the `data-on` attribute
//! (`%["","count"," selected"]`), so the browser formats it with no catalog and no round trip.
//!
//! ```
//! corten::i18n::messages! { struct Msg { "en" => "src/i18n/en.json" } }
//! use corten::script::{message_id, ts::Message, Arg};
//! fn wants_a_count(_: impl Arg<Message<{ message_id(&["count"]) }>>) {}
//! wants_a_count(Msg::default().js().item_listing_count()); // "{count} listings"
//! ```
//!
//! A message whose arguments are not the ones the JSDoc names does not compile:
//!
//! ```compile_fail
//! corten::i18n::messages! { struct Msg { "en" => "src/i18n/en.json" } }
//! fn wants_a_count(_: impl corten::script::Arg<corten::script::ts::Message<{ corten::script::message_id(&["count"]) }>>) {}
//! wants_a_count(Msg::default().js().item_image()); // takes {title} and {n}
//! ```
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

mod display_digits;
mod format;

pub use format::{
    parse_timestamp, Amount, Currency, DateStyle, Decimal, Formatted, InvalidDecimal, Minor, Number, Offset, TimeStyle, Utc,
    When, Zone,
};

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

/// A message with arguments, unfilled (`t.js().key()`): the argument a handler whose JSDoc says
/// `Message<"a" | "b">` takes, when the message's arguments are exactly `a` and `b` (`ARGS` is
/// [`crate::script::message_id`] of their names). Its text is made when the crate compiles.
#[derive(Clone, Copy, Debug)]
pub struct Template<const ARGS: u64>(&'static str);

impl<const ARGS: u64> Template<ARGS> {
    #[doc(hidden)]
    pub const fn new(wire: &'static str) -> Self {
        Template(wire)
    }

    /// The text `data-on` carries, HTML-escaped.
    pub(crate) fn wire(self) -> &'static str {
        self.0
    }
}

/// The arguments of a message found by its key (`by_key`, from [`messages!`]), by name: pairs
/// (`[("count", 3)]`), a map, or a JSON object (feature `json`; a string is written without its
/// quotes, any other value as its JSON).
pub trait Args {
    /// Calls `write` with the argument `name` and returns true, or returns false when there is none.
    fn arg(&self, name: &str, write: &mut dyn FnMut(&dyn Display)) -> bool;
}

impl<K: AsRef<str>, V: Display> Args for [(K, V)] {
    fn arg(&self, name: &str, write: &mut dyn FnMut(&dyn Display)) -> bool {
        self.iter().find(|(k, _)| k.as_ref() == name).map(|(_, v)| write(v)).is_some()
    }
}

impl<K: AsRef<str>, V: Display, const N: usize> Args for [(K, V); N] {
    fn arg(&self, name: &str, write: &mut dyn FnMut(&dyn Display)) -> bool {
        self.as_slice().arg(name, write)
    }
}

impl<K: AsRef<str>, V: Display> Args for Vec<(K, V)> {
    fn arg(&self, name: &str, write: &mut dyn FnMut(&dyn Display)) -> bool {
        self.as_slice().arg(name, write)
    }
}

impl<K: std::borrow::Borrow<str> + Ord, V: Display> Args for std::collections::BTreeMap<K, V> {
    fn arg(&self, name: &str, write: &mut dyn FnMut(&dyn Display)) -> bool {
        self.get(name).map(|v| write(v)).is_some()
    }
}

impl<K: std::borrow::Borrow<str> + Eq + std::hash::Hash, V: Display, S: std::hash::BuildHasher> Args
    for std::collections::HashMap<K, V, S>
{
    fn arg(&self, name: &str, write: &mut dyn FnMut(&dyn Display)) -> bool {
        self.get(name).map(|v| write(v)).is_some()
    }
}

#[cfg(feature = "json")]
impl Args for serde_json::Map<String, serde_json::Value> {
    fn arg(&self, name: &str, write: &mut dyn FnMut(&dyn Display)) -> bool {
        match self.get(name) {
            Some(serde_json::Value::String(text)) => write(text),
            Some(value) => write(value),
            None => return false,
        }
        true
    }
}

/// An object's members (another value has none).
#[cfg(feature = "json")]
impl Args for serde_json::Value {
    fn arg(&self, name: &str, write: &mut dyn FnMut(&dyn Display)) -> bool {
        self.as_object().is_some_and(|members| members.arg(name, write))
    }
}

impl<A: Args + ?Sized> Args for &A {
    fn arg(&self, name: &str, write: &mut dyn FnMut(&dyn Display)) -> bool {
        (**self).arg(name, write)
    }
}

/// A message found by its key, with its arguments: what `by_key` returns. In `html!` it renders
/// escaped; `to_string()` gives the plain text.
pub struct Found<'a, A: ?Sized> {
    pieces: &'static [__private::Piece],
    args: &'a A,
}

impl<'a, A: Args + ?Sized> Found<'a, A> {
    /// The message, or `None` when `args` lacks one of its arguments.
    #[doc(hidden)]
    pub fn new(pieces: &'static [__private::Piece], args: &'a A) -> Option<Self> {
        let complete = pieces.iter().all(|piece| match piece {
            __private::Piece::Arg(name) => args.arg(name, &mut |_| {}),
            __private::Piece::Text(..) => true,
        });
        complete.then_some(Found { pieces, args })
    }

    fn write(&self, sink: &mut Sink<'_>) {
        for piece in self.pieces {
            match piece {
                __private::Piece::Text(plain, html) => sink.text(plain, html),
                __private::Piece::Arg(name) => {
                    self.args.arg(name, &mut |value| sink.arg(value));
                }
            }
        }
    }
}

impl<A: Args + ?Sized> Render for Found<'_, A> {
    fn render(&self, out: &mut String) {
        self.write(&mut Sink { out, html: true })
    }
}

impl<A: Args + ?Sized> AttrValue for Found<'_, A> {
    fn write_attr(&self, name: &str, out: &mut String) {
        out.push_str(name);
        out.push_str("=\"");
        self.render(out);
        out.push('"');
    }
}

impl<A: Args + ?Sized> Display for Found<'_, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        self.write(&mut Sink { out: &mut out, html: false });
        f.write_str(&out)
    }
}

/// Used by the code `messages!` generates. Not a public API.
#[doc(hidden)]
pub mod __private {
    pub use super::format::__private::{Field, LocaleData, Separators};

    /// A run of a message: its text (plain and escaped) or an argument's name.
    pub enum Piece {
        Text(&'static str, &'static str),
        Arg(&'static str),
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
        let view = crate::html! {
            <img alt={fr.item_image("A & B", 1)}>
            <p>{en.item_listing_count(1)}</p>
        };
        assert_eq!(view.to_html(), r#"<img alt="Image 1 de A &amp; B"><p>1 listings</p>"#);
        assert_eq!(Msg::for_locale("FR"), fr);
        assert_eq!(Msg::default().locale(), "en");
        // Arguments named like Rust keywords are raw parameters; the catalog keeps its names.
        assert_eq!(en.attention_detail("TCGplayer", "alias", 0.9).to_string(), "TCGplayer · alias · confidence 0.9");
        let by_key = fr.by_key("attention.detail", &[("provider", "P"), ("type", "alias"), ("match", "1")]);
        assert_eq!(by_key.unwrap().to_string(), "P · alias · confiance 1");
        // camelCase arguments: snake case parameters, the catalog's names by key.
        assert_eq!(en.attention_updated("today", "<Pika>").to_html(), "Updated today by &lt;Pika&gt;");
        let params = [("updatedAt", "hier"), ("itemTitle", "Pika")];
        assert_eq!(fr.by_key("attention.updated", &params).unwrap().to_string(), "Mis à jour hier par Pika");
        assert!(fr.by_key("attention.updated", &[("updated_at", "x"), ("item_title", "y")]).is_none(), "by the catalog's names");
    }

    #[test]
    fn a_message_is_found_by_its_catalog_key_with_named_arguments() {
        let (en, fr) = (Msg::for_locale("en"), Msg::for_locale("fr"));
        let args = [("title", "<Pika>"), ("n", "2")];
        assert_eq!(en.by_key("item.image", &args).unwrap().to_html(), "&lt;Pika&gt; image 2", "escaped in a page");
        assert_eq!(fr.by_key("item.image", &args).unwrap().to_string(), "Image 2 de <Pika>");
        assert_eq!(en.by_key("item.listings", &[("unused", 1)]).unwrap().to_string(), "Listings", "no arguments");
        assert_eq!(en.by_key("brace", &[("x", 0)]).unwrap().to_html(), "{literal} &amp; more");
        assert!(en.by_key("item_listings", &args).is_none(), "the catalog key, not the method name");
        assert!(en.by_key("item.listing_count", &[("total", 3)]).is_none(), "an argument is missing");
        let mut map = std::collections::HashMap::new();
        map.insert("count".to_string(), 3);
        assert_eq!(fr.by_key("item.listing_count", &map).unwrap().to_string(), "3 annonces");
        #[cfg(feature = "json")]
        {
            let answer: serde_json::Value = serde_json::from_str(r#"{"titleParams":{"title":"A & B","n":4}}"#).unwrap();
            let view = crate::html! { <p>{en.by_key("item.image", &answer["titleParams"])}</p> };
            assert_eq!(view.to_html(), "<p>A &amp; B image 4</p>", "a JSON string without quotes, a number as written");
        }
    }

    /// Every case `scripts/locale-data.mjs` had Intl write, for every locale: numbers, money and
    /// dates in fixed zones.
    #[test]
    fn every_locale_writes_numbers_money_and_dates_as_intl_does() {
        let locales: &[(&str, &__private::LocaleData)] = corten_macros::__all_locales!();
        let mut checked = 0;
        for line in include_str!("i18n/cases.tsv").lines().filter(|l| !l.starts_with('#') && !l.is_empty()) {
            let mut cols = line.split('\t');
            let (tag, kind, input, expected) =
                (cols.next().unwrap(), cols.next().unwrap(), cols.next().unwrap(), cols.next().unwrap());
            let data = locales.iter().find(|(t, _)| *t == tag).expect(tag).1;
            let ours = match kind.split(' ').collect::<Vec<_>>()[..] {
                ["number"] => Formatted::number(data, Decimal::parse(input).unwrap()).to_string(),
                ["money"] => {
                    let (amount, code) = input.split_once(' ').unwrap();
                    Formatted::money(data, &Decimal::parse(amount).unwrap(), Currency::parse(code).unwrap()).to_string()
                }
                ["date", style, ref offset @ ..] => {
                    let at = parse_timestamp(input).unwrap();
                    let style: u8 = style.parse().unwrap();
                    let styles = [DateStyle::Short, DateStyle::Medium, DateStyle::Long, DateStyle::Full];
                    let times = [TimeStyle::Short, TimeStyle::Medium];
                    let when = match style {
                        0..=3 => When::date(data, at, styles[style as usize]),
                        4..=11 => When::date_time(data, at, styles[(style as usize - 4) / 2], times[(style as usize - 4) % 2]),
                        _ => When::time(data, at, times[style as usize - 12]),
                    };
                    let offset = offset.first().map_or(0, |o| o.parse().unwrap());
                    when.zone(Offset::seconds(offset)).to_string()
                }
                _ => panic!("{line}"),
            };
            assert_eq!(ours, expected, "{tag} {kind} {input}");
            checked += 1;
        }
        assert!(checked > 9000, "{checked}");
    }

    mod regional {
        crate::i18n::messages! { pub struct Msg { "en-GB" => "src/i18n/en.json", "fr-LU" => "src/i18n/fr.json" } }
    }

    #[test]
    fn a_region_has_its_own_data_or_its_languages() {
        use regional::Msg;
        let at = parse_timestamp("2026-03-22T12:00:00Z").unwrap();
        assert_eq!(Msg::for_locale("en-GB").date(at, DateStyle::Short).to_string(), "22/03/2026", "en-GB's own");
        assert_eq!(Msg::for_locale("fr-LU").date(at, DateStyle::Long).to_string(), "22 mars 2026", "fr's");
    }

    #[test]
    fn decimals_round_half_away_from_zero_and_timestamps_parse_as_rfc_3339() {
        let d = |s| Decimal::parse(s).unwrap();
        assert_eq!(d("21.505").in_digits(2).to_string(), "21.51");
        assert_eq!(d("-21.505").in_digits(2).to_string(), "-21.51");
        assert_eq!(d("21.5049").in_digits(2).to_string(), "21.50");
        assert_eq!(d("9.995").in_digits(2).to_string(), "10.00");
        assert_eq!(d(".5").in_digits(0).to_string(), "1");
        assert_eq!(d("3").in_digits(3).to_string(), "3.000");
        assert_eq!(Minor(-5).in_digits(2).to_string(), "-0.05");
        assert_eq!(d("-0.25").to_string(), "-0.25");
        for bad in ["", "-", ".", "1e5", "1,5", " 1", "1.2.3", &"9".repeat(39)] {
            assert_eq!(Decimal::parse(bad), None, "{bad:?}");
        }
        assert_eq!("21.5".parse::<Decimal>(), Ok(Decimal::new(215, 1)));
        assert_eq!(Currency::parse("usd"), Some(Currency::new("USD")));
        assert_eq!(Currency::parse("US"), None);
        assert_eq!((Currency::new("JPY").digits(), Currency::new("KWD").digits(), Currency::new("XYZ").digits()), (0, 3, 2));

        let secs = |s| parse_timestamp(s).map(super::format::unix_seconds_for_test);
        assert_eq!(secs("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(secs("2026-03-22T12:00:00.000Z"), Some(1_774_180_800));
        assert_eq!(secs("2026-03-22t17:30:00+05:30"), Some(1_774_180_800));
        assert_eq!(secs("2026-03-22 07:00:00.123456-05"), Some(1_774_180_800));
        assert_eq!(secs("2026-03-22T07:00:00-0500"), Some(1_774_180_800));
        assert_eq!(secs("1969-12-31T23:59:59Z"), Some(-1));
        assert!(secs("2024-02-29T00:00:00Z").is_some());
        for bad in [
            "2026-02-29T00:00:00Z",
            "2026-13-01T00:00:00Z",
            "2026-03-22T24:00:00Z",
            "2026-03-22",
            "2026-03-22T12:00:00",
            "2026-03-22T12:00:00Zx",
            "2026-03-22T12:00:00+5",
        ] {
            assert_eq!(parse_timestamp(bad), None, "{bad}");
        }
    }

    #[test]
    fn numbers_money_and_dates_render_in_a_page_and_as_arguments() {
        let (en, fr) = (Msg::for_locale("en"), Msg::for_locale("fr"));
        let usd = Currency::new("USD");
        let at = parse_timestamp("2026-03-22T12:00:00Z").unwrap();
        let view = crate::html! {
            <p title={fr.money(Minor(-123450), usd)}>{en.item_listing_count(en.number(1234))}</p>
            <time>{en.date_time(at, DateStyle::Medium, TimeStyle::Short)}</time>
        };
        assert_eq!(
            view.to_html(),
            "<p title=\"-1\u{202f}234,50\u{a0}$US\">1,234 listings</p><time>Mar 22, 2026, 12:00 PM UTC</time>"
        );
        assert_eq!(fr.number(Decimal::parse("-1234.5").unwrap()).to_string(), "-1\u{202f}234,5");
        assert_eq!(
            en.date(std::time::UNIX_EPOCH - std::time::Duration::from_secs(1), DateStyle::Full).to_string(),
            "Wednesday, December 31, 1969"
        );
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
