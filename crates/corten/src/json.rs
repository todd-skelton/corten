//! [`Json`]: a JSON response or request body, [`ToJson`]: a value that writes itself as JSON, and
//! [`FromJson`]: a value read from JSON by Corten's reader, with serde_json where it gives up.
//!
//! `#[derive(ToJson)]` writes a struct or an enum as `serde_json` would, with the layout decided when
//! the crate compiles: the keys, quotes, colons and commas between two values are one byte string,
//! copied into the response as it is, so only the values are written per request. A request for
//! the item page's view model as JSON costs half the instructions it did through serde_json
//! (see the changelog). The response is written into a `Vec` the body takes as it is, sized from
//! what the type wrote last time.
//!
//! ```
//! use corten::prelude::*;
//!
//! #[derive(ToJson)]
//! struct Price { id: String, lowest: &'static str }
//!
//! async fn price(Path(id): Path<String>) -> Json<Price> {
//!     Json(Price { id, lowest: "$0.50" })
//! }
//! ```
//!
//! The derive reads `#[serde(...)]`, so a type that also derives `Serialize` or `Deserialize` writes
//! what serde writes and reads back what it wrote: `rename`, `rename_all`, `skip`,
//! `skip_serializing`, `skip_serializing_if` and `transparent`, and on an enum `tag`, `content` and
//! `untagged` (serde's default, a variant's name as the key, otherwise), and `flatten` of a struct
//! that derives `ToJson` ([`ToJsonMembers`]). One that changes what serde writes and the derive
//! cannot follow (`flatten` of a map, `serialize_with`, `into` ...) is a compile error rather
//! than different JSON:
//!
//! ```compile_fail
//! #[derive(corten::ToJson)]
//! struct Page {
//!     #[serde(flatten)]
//!     extra: std::collections::HashMap<String, String>,
//! }
//! ```
//!
//! ```compile_fail
//! #[derive(corten::ToJson)]
//! struct Named { id: u32 }
//! #[derive(corten::ToJson)]
//! struct Page {
//!     #[serde(flatten, rename = "named")]
//!     named: Named,
//! }
//! ```
//!
//! A field's type implements `ToJson` (strings, numbers, `bool`, `Option`, lists, maps with string
//! or integer keys, tuples, `Box`/`Rc`/`Arc`, `serde_json::Value` and derived types), or else
//! `serde::Serialize`: a type from another crate (an id, a date) is written by serde_json, with the
//! same output, at serde_json's cost. A `Serialize` impl that fails (a map with a non-string key
//! inside such a type) panics, since a value that cannot be JSON is a bug in the type.
//!
//! What a response writes must be `ToJson`, so a type that only derives `Serialize` does not
//! compile as one, and neither does a map whose keys JSON cannot hold:
//!
//! ```compile_fail
//! use corten::{axum::response::IntoResponse, Json};
//! #[derive(serde::Serialize)]
//! struct Price { lowest: String }
//! let _ = Json(Price { lowest: "$1".into() }).into_response();
//! ```
//!
//! ```compile_fail
//! use corten::{axum::response::IntoResponse, Json};
//! let mut grid = std::collections::BTreeMap::new();
//! grid.insert((1, 2), "a tuple is not a JSON key");
//! let _ = Json(grid).into_response();
//! ```
//!
//! As an extractor `Json` is axum's, with the same rejections (under [`crate::App::api`] they are
//! answered as JSON errors).
//!
//! # Reading
//!
//! [`from_slice`] and [`from_str`] read a [`FromJson`] type, which `#[derive(FromJson)]` implements next to
//! serde's `Deserialize`: another service's response, read into structs of only the fields a page
//! uses. Corten's reader matches each key against the fields, makes each value straight from the
//! text and skips (validating) the rest; where it meets anything it does not follow (an escaped
//! key, a missing or duplicate field, a number it does not parse itself) serde_json reads the text
//! instead, so the value, and every error, is `serde_json::from_slice`'s. The item page's 55 KB API
//! response takes a fifth fewer instructions than through serde_json (see the changelog).
//!
//! ```
//! use corten::FromJson;
//!
//! #[derive(serde::Deserialize, FromJson, Debug, PartialEq)]
//! #[serde(rename_all = "camelCase")]
//! struct Listing { listing_id: String, price: Option<f64> }
//!
//! let listing: Listing = corten::json::from_str(r#"{"listingId":"l1","seller":{"id":7},"price":2.5}"#).unwrap();
//! assert_eq!(listing, Listing { listing_id: "l1".into(), price: Some(2.5) });
//! ```
//!
//! It reads `rename`, `rename_all`, `alias`, `default`, `skip`, `skip_deserializing`,
//! `deny_unknown_fields` and `transparent`, on structs with named fields, tuple and unit structs, and
//! enums of unit variants. A field's type is a `FromJson` (strings, numbers, `bool`, `char`, `Option`,
//! `Vec`, `Box`, maps with `String` keys, `serde_json::Value` and derived types) or else any
//! `Deserialize` type, which serde_json reads from that field's text. An attribute that changes how
//! serde reads in a way the derive does not follow, or an enum with data, does not compile; derive
//! `Deserialize` alone for that type and it is still read, by serde_json, as a field:
//!
//! ```compile_fail
//! #[derive(serde::Deserialize, corten::FromJson)]
//! struct Page {
//!     #[serde(flatten)]
//!     extra: std::collections::HashMap<String, String>,
//! }
//! ```
//!
//! ```compile_fail
//! #[derive(serde::Deserialize, corten::FromJson)]
//! enum Event { Sold { price: f64 } }
//! ```
//!
//! A type that only derives `FromJson` does not compile either: serde's `Deserialize` is the fallback.
//!
//! ```compile_fail
//! #[derive(corten::FromJson)]
//! struct Price { amount: f64 }
//! ```

use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    rc::Rc,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use axum::{
    body::{Body, Bytes},
    extract::{rejection::JsonRejection, FromRequest, Request},
    http::{header, HeaderValue},
    response::{IntoResponse, Response},
};

pub use corten_macros::{FromJson, ToJson};

mod read;
pub use read::{from_slice, from_str, FromJson};

/// Used by the code `#[derive(FromJson)]` generates. Not a public API.
#[doc(hidden)]
pub use read::__private as __read;

/// JSON out (`application/json`) or in (a request body with a JSON content type).
#[derive(Clone, Copy, Debug, Default)]
#[must_use]
pub struct Json<T>(pub T);

impl<T: ToJson> IntoResponse for Json<T> {
    fn into_response(self) -> Response {
        let mut res = self.0.__into_response();
        // Room for the headers `App` adds too, so the map is allocated once.
        res.headers_mut().reserve(12);
        res.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static(crate::api::JSON));
        res
    }
}

/// `value`'s JSON, in a vector sized from what its type wrote last time.
pub fn to_vec<T: ToJson + ?Sized>(value: &T) -> Vec<u8> {
    let size = T::__json_size();
    let mut out = Vec::with_capacity(size.map_or(128, |s| s.load(Ordering::Relaxed).max(128)));
    value.write_json(&mut out);
    if let Some(size) = size {
        size.store(out.len() + out.len() / 8, Ordering::Relaxed);
    }
    out
}

/// A value that writes itself as JSON: what `#[derive(ToJson)]` implements, and what a [`Json`]
/// response holds. A hand-written impl appends exactly one JSON value to `out`.
pub trait ToJson {
    /// Appends the value's JSON to `out`.
    fn write_json(&self, out: &mut Vec<u8>);

    /// The value's JSON.
    fn to_json(&self) -> String {
        String::from_utf8(to_vec(self)).expect("ToJson writes UTF-8")
    }

    /// The size this type wrote last, for the derive (one per type). Not a public API.
    #[doc(hidden)]
    fn __json_size() -> Option<&'static AtomicUsize> {
        None
    }

    /// The response [`Json`] sends, before its headers: the value written into a new vector, or
    /// for [`Raw`] the bytes it holds. Not a public API.
    #[doc(hidden)]
    fn __into_response(self) -> Response
    where
        Self: Sized,
    {
        // `Body::from(Vec)` keeps the vector's allocation: no copy.
        Response::new(Body::from(to_vec(&self)))
    }
}

/// JSON the app already holds as bytes: a document built once when its data changes and read by
/// many requests, or a file sent as it was written. `Json(raw.clone())` sends the bytes as they
/// are (a clone shares them), with no parse and no copy; as a field of a value `Json` writes, they
/// are copied in as they are. `Raw::new` reads the bytes through once to check they are one JSON
/// value (a corrupt file is an error, not a broken answer), so make one per document, not per
/// request; and clone it, not an `Arc<Raw>`, which `Json` writes as any value.
///
/// With feature `compress`, a `Raw` sent a second time is compressed once with brotli and gzip
/// (levels for a value that changes while the app runs, made off the request), and later reads
/// send those forms, so a held document is not compressed again for every client. Keep the
/// `Raw` and clone it for each request; a `Raw` made for one request is compressed as any other
/// answer.
///
/// ```
/// use std::time::Duration;
/// use corten::{json::Raw, prelude::*};
///
/// #[derive(Clone)]
/// struct Ladders(Raw);
///
/// async fn ladders(State(Ladders(doc)): State<Ladders>) -> Public<JsonResult<Raw>> {
///     let answer: JsonResult<Raw> = Ok(Json(doc));
///     Public::new(Duration::from_secs(60), answer)
/// }
///
/// # fn main() -> Result<(), corten::json::InvalidJson> {
/// let doc = Ladders(Raw::new(r#"{"ladders":[]}"#)?);
/// assert_eq!(doc.0.as_bytes(), br#"{"ladders":[]}"#);
/// assert!(Raw::new(r#"{"ladders":["#).is_err());
/// # let _ = ladders;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct Raw(#[cfg(feature = "compress")] Arc<crate::compress::Precompressed>, #[cfg(not(feature = "compress"))] Bytes);

/// Bytes [`Raw::new`] was given that are not one JSON value in UTF-8, and where they stop being
/// one.
#[derive(Debug)]
pub struct InvalidJson(InvalidJsonKind);

#[derive(Debug)]
enum InvalidJsonKind {
    Utf8(std::str::Utf8Error),
    Json(serde_json::Error),
}

impl std::fmt::Display for InvalidJson {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            InvalidJsonKind::Utf8(error) => write!(f, "not one JSON value: not UTF-8, {error}"),
            InvalidJsonKind::Json(error) => write!(f, "not one JSON value: {error}"),
        }
    }
}

impl std::error::Error for InvalidJson {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.0 {
            InvalidJsonKind::Utf8(error) => Some(error),
            InvalidJsonKind::Json(error) => Some(error),
        }
    }
}

impl Raw {
    /// The JSON in `bytes`, or an error when they are not one JSON value in UTF-8 (a byte order
    /// mark included: JSON sent over a network has none).
    pub fn new(bytes: impl Into<Bytes>) -> Result<Raw, InvalidJson> {
        let bytes = bytes.into();
        // UTF-8 first: the parser skips a string it ignores without checking its bytes.
        let text = std::str::from_utf8(&bytes).map_err(|e| InvalidJson(InvalidJsonKind::Utf8(e)))?;
        serde_json::from_str::<serde::de::IgnoredAny>(text).map_err(|e| InvalidJson(InvalidJsonKind::Json(e)))?;
        #[cfg(feature = "compress")]
        return Ok(Raw(Arc::new(crate::compress::Precompressed::held(bytes))));
        #[cfg(not(feature = "compress"))]
        Ok(Raw(bytes))
    }

    /// The JSON as it is held.
    pub fn as_bytes(&self) -> &[u8] {
        self.bytes()
    }

    fn bytes(&self) -> &Bytes {
        #[cfg(feature = "compress")]
        return self.0.identity();
        #[cfg(not(feature = "compress"))]
        &self.0
    }
}

#[cfg(all(test, feature = "compress"))]
impl Raw {
    pub(crate) fn stored_started(&self) -> bool {
        self.0.started()
    }

    pub(crate) fn wait_stored(&self) {
        self.0.wait()
    }
}

impl std::fmt::Debug for Raw {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The length and the start: a document can be large.
        let bytes = self.as_bytes();
        let start = String::from_utf8_lossy(&bytes[..bytes.len().min(64)]);
        f.debug_struct("Raw").field("len", &bytes.len()).field("start", &start).finish()
    }
}

impl ToJson for Raw {
    fn write_json(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(self.as_bytes());
    }

    fn __into_response(self) -> Response {
        #[cfg_attr(not(feature = "compress"), allow(unused_mut))]
        let mut res = Response::new(Body::from(self.bytes().clone()));
        #[cfg(feature = "compress")]
        res.extensions_mut().insert(crate::compress::Stored(self.0));
        res
    }
}

/// A struct whose members another object can hold: what a `#[serde(flatten)]` field writes.
/// `#[derive(ToJson)]` implements it for a struct with named fields, so
///
/// ```
/// #[derive(serde::Serialize, corten::ToJson)]
/// struct Dimension { id: u32, name: String }
///
/// #[derive(serde::Serialize, corten::ToJson)]
/// struct DimensionDetail {
///     #[serde(flatten)]
///     dimension: Dimension,
///     options: Vec<String>,
/// }
///
/// use corten::json::ToJson;
/// let detail = DimensionDetail { dimension: Dimension { id: 7, name: "Size".into() }, options: vec![] };
/// assert_eq!(detail.to_json(), r#"{"id":7,"name":"Size","options":[]}"#);
/// ```
///
/// writes the dimension's members in the detail's braces, as serde does, with the layout still
/// decided when the crate compiles. `Option` and `Box` of such a struct flatten too (`None`
/// writes nothing). Flattening anything else, a map or an enum, which serde lays out only when it
/// runs, does not compile:
///
/// ```compile_fail
/// #[derive(serde::Serialize, corten::ToJson)]
/// enum Kind { Size, Color }
///
/// #[derive(serde::Serialize, corten::ToJson)]
/// struct Detail {
///     #[serde(flatten)]
///     kind: Kind,
/// }
/// ```
#[diagnostic::on_unimplemented(
    message = "`#[serde(flatten)]` needs a struct with named fields that derives ToJson; `{Self}` is not one",
    label = "flattened here",
    note = "a map or an enum is laid out only when serde runs; implement ToJson by hand for the outer type"
)]
pub trait ToJsonMembers {
    /// Appends the members, `"key":value` each, without braces: a comma before the first one
    /// unless `first` (nothing is in the object yet). Returns whether the object is still empty.
    fn write_json_members(&self, out: &mut Vec<u8>, first: bool) -> bool;
}

impl<T: ToJsonMembers> ToJsonMembers for Option<T> {
    #[inline]
    fn write_json_members(&self, out: &mut Vec<u8>, first: bool) -> bool {
        match self {
            Some(value) => value.write_json_members(out, first),
            None => first,
        }
    }
}

impl<T: ToJsonMembers + ?Sized> ToJsonMembers for Box<T> {
    #[inline]
    fn write_json_members(&self, out: &mut Vec<u8>, first: bool) -> bool {
        (**self).write_json_members(out, first)
    }
}

/// A map key: written as a JSON string (numbers, `bool` and `char` in quotes, as serde_json does).
pub trait JsonKey {
    fn write_key(&self, out: &mut Vec<u8>);
}

/// The escaped contents of a JSON string, without its quotes: what serde_json escapes (`"`, `\` and
/// control characters, `\n`-style where JSON has a short form), nothing else.
#[inline]
fn str_body(out: &mut Vec<u8>, s: &str) {
    if crate::ascii::any(s.as_bytes(), crate::ascii::json) {
        escaped(out, s.as_bytes());
    } else {
        out.extend_from_slice(s.as_bytes());
    }
}

#[cold]
fn escaped(out: &mut Vec<u8>, bytes: &[u8]) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        let short: &[u8] = match b {
            b'"' => b"\\\"",
            b'\\' => b"\\\\",
            b'\n' => b"\\n",
            b'\r' => b"\\r",
            b'\t' => b"\\t",
            0x08 => b"\\b",
            0x0c => b"\\f",
            0..0x20 => b"",
            _ => continue,
        };
        out.extend_from_slice(&bytes[start..i]);
        if short.is_empty() {
            out.extend_from_slice(&[b'\\', b'u', b'0', b'0', HEX[(b >> 4) as usize], HEX[(b & 15) as usize]]);
        } else {
            out.extend_from_slice(short);
        }
        start = i + 1;
    }
    out.extend_from_slice(&bytes[start..]);
}

#[inline]
fn string(out: &mut Vec<u8>, s: &str) {
    out.push(b'"');
    str_body(out, s);
    out.push(b'"');
}

#[inline]
fn unsigned(out: &mut Vec<u8>, n: u64) {
    out.extend_from_slice(crate::ascii::digits(n, &mut [0; 20]));
}

#[inline]
fn signed(out: &mut Vec<u8>, n: i64) {
    if n < 0 {
        out.push(b'-');
    }
    unsigned(out, n.unsigned_abs());
}

impl ToJson for str {
    #[inline]
    fn write_json(&self, out: &mut Vec<u8>) {
        string(out, self)
    }
}

impl ToJson for String {
    #[inline]
    fn write_json(&self, out: &mut Vec<u8>) {
        string(out, self)
    }
}

impl ToJson for Cow<'_, str> {
    #[inline]
    fn write_json(&self, out: &mut Vec<u8>) {
        string(out, self)
    }
}

impl ToJson for char {
    #[inline]
    fn write_json(&self, out: &mut Vec<u8>) {
        string(out, self.encode_utf8(&mut [0; 4]))
    }
}

impl ToJson for bool {
    #[inline]
    fn write_json(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(if *self { b"true" } else { b"false" })
    }
}

macro_rules! integers {
    ($write:ident as $wide:ty: $($t:ty),*) => {$(
        impl ToJson for $t {
            #[inline]
            fn write_json(&self, out: &mut Vec<u8>) {
                $write(out, *self as $wide)
            }
        }

        impl JsonKey for $t {
            #[inline]
            fn write_key(&self, out: &mut Vec<u8>) {
                out.push(b'"');
                $write(out, *self as $wide);
                out.push(b'"');
            }
        }
    )*};
}
integers!(unsigned as u64: u8, u16, u32, u64, usize);
integers!(signed as i64: i8, i16, i32, i64, isize);

macro_rules! wide_integers {
    ($($t:ty),*) => {$(
        impl ToJson for $t {
            fn write_json(&self, out: &mut Vec<u8>) {
                out.extend_from_slice(self.to_string().as_bytes())
            }
        }

        impl JsonKey for $t {
            fn write_key(&self, out: &mut Vec<u8>) {
                string(out, &self.to_string())
            }
        }
    )*};
}
wide_integers!(u128, i128);

macro_rules! floats {
    ($($t:ty),*) => {$(
        /// The shortest digits that read back as the same number; NaN and infinity, which JSON
        /// has no form for, are `null` (as serde_json writes them).
        impl ToJson for $t {
            #[inline]
            fn write_json(&self, out: &mut Vec<u8>) {
                if self.is_finite() {
                    out.extend_from_slice(zmij::Buffer::new().format_finite(*self).as_bytes())
                } else {
                    out.extend_from_slice(b"null")
                }
            }
        }
    )*};
}
floats!(f32, f64);

impl ToJson for () {
    #[inline]
    fn write_json(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(b"null")
    }
}

impl<T: ToJson> ToJson for Option<T> {
    #[inline]
    fn write_json(&self, out: &mut Vec<u8>) {
        match self {
            Some(value) => value.write_json(out),
            None => out.extend_from_slice(b"null"),
        }
    }
}

macro_rules! forward {
    ($($t:ty),*) => {$(
        impl<T: ToJson + ?Sized> ToJson for $t {
            #[inline]
            fn write_json(&self, out: &mut Vec<u8>) {
                (**self).write_json(out)
            }

            fn __json_size() -> Option<&'static AtomicUsize> {
                T::__json_size()
            }
        }
    )*};
}
forward!(&T, &mut T, Box<T>, Rc<T>, Arc<T>);

/// `[a,b,c]` from the items in order.
#[inline]
fn array<'a, T: ToJson + 'a>(out: &mut Vec<u8>, items: impl IntoIterator<Item = &'a T>) {
    out.push(b'[');
    let mut items = items.into_iter();
    if let Some(first) = items.next() {
        first.write_json(out);
        for item in items {
            out.push(b',');
            item.write_json(out);
        }
    }
    out.push(b']');
}

impl<T: ToJson> ToJson for [T] {
    #[inline]
    fn write_json(&self, out: &mut Vec<u8>) {
        array(out, self)
    }
}

impl<T: ToJson, const N: usize> ToJson for [T; N] {
    #[inline]
    fn write_json(&self, out: &mut Vec<u8>) {
        array(out, self)
    }
}

impl<T: ToJson> ToJson for Vec<T> {
    #[inline]
    fn write_json(&self, out: &mut Vec<u8>) {
        array(out, self)
    }
}

impl<T: ToJson> ToJson for VecDeque<T> {
    fn write_json(&self, out: &mut Vec<u8>) {
        array(out, self)
    }
}

impl<T: ToJson> ToJson for BTreeSet<T> {
    fn write_json(&self, out: &mut Vec<u8>) {
        array(out, self)
    }
}

impl<T: ToJson, S> ToJson for HashSet<T, S> {
    fn write_json(&self, out: &mut Vec<u8>) {
        array(out, self)
    }
}

macro_rules! tuples {
    ($(($first:ident $(, $rest:ident)*)),*) => {$(
        /// An array, as serde writes a tuple.
        impl<$first: ToJson $(, $rest: ToJson)*> ToJson for ($first, $($rest,)*) {
            #[allow(non_snake_case)]
            fn write_json(&self, out: &mut Vec<u8>) {
                let ($first, $($rest,)*) = self;
                out.push(b'[');
                $first.write_json(out);
                $(
                    out.push(b',');
                    $rest.write_json(out);
                )*
                out.push(b']');
            }
        }
    )*};
}
tuples!((A), (A, B), (A, B, C), (A, B, C, D), (A, B, C, D, E), (A, B, C, D, E, F));

impl JsonKey for str {
    #[inline]
    fn write_key(&self, out: &mut Vec<u8>) {
        string(out, self)
    }
}

impl JsonKey for String {
    #[inline]
    fn write_key(&self, out: &mut Vec<u8>) {
        string(out, self)
    }
}

impl JsonKey for Cow<'_, str> {
    #[inline]
    fn write_key(&self, out: &mut Vec<u8>) {
        string(out, self)
    }
}

impl JsonKey for char {
    fn write_key(&self, out: &mut Vec<u8>) {
        string(out, self.encode_utf8(&mut [0; 4]))
    }
}

impl JsonKey for bool {
    fn write_key(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(if *self { b"\"true\"" } else { b"\"false\"" })
    }
}

macro_rules! forward_key {
    ($($t:ty),*) => {$(
        impl<K: JsonKey + ?Sized> JsonKey for $t {
            #[inline]
            fn write_key(&self, out: &mut Vec<u8>) {
                (**self).write_key(out)
            }
        }
    )*};
}
forward_key!(&K, Box<K>, Rc<K>, Arc<K>);

/// `{"k":v,...}` from the entries in order.
#[inline]
fn object<'a, K: JsonKey + ?Sized + 'a, V: ToJson + 'a>(out: &mut Vec<u8>, entries: impl IntoIterator<Item = (&'a K, &'a V)>) {
    out.push(b'{');
    let mut first = true;
    for (key, value) in entries {
        if !first {
            out.push(b',');
        }
        first = false;
        key.write_key(out);
        out.push(b':');
        value.write_json(out);
    }
    out.push(b'}');
}

impl<K: JsonKey, V: ToJson> ToJson for BTreeMap<K, V> {
    fn write_json(&self, out: &mut Vec<u8>) {
        object(out, self)
    }
}

/// In the map's order, as serde_json writes it.
impl<K: JsonKey, V: ToJson, S> ToJson for HashMap<K, V, S> {
    fn write_json(&self, out: &mut Vec<u8>) {
        object(out, self)
    }
}

macro_rules! serde_json_values {
    ($($t:ty),*) => {$(
        /// Written by serde_json (it cannot fail for a `Value`).
        impl ToJson for $t {
            fn write_json(&self, out: &mut Vec<u8>) {
                __private::serde(self, out)
            }
        }
    )*};
}
serde_json_values!(serde_json::Value, serde_json::Map<String, serde_json::Value>, serde_json::Number);

/// Used by the code `#[derive(ToJson)]` generates. Not a public API.
#[doc(hidden)]
pub mod __private {
    use serde::Serialize;

    use super::ToJson;

    /// A string value whose quotes the derive wrote around it.
    #[inline]
    pub fn str_body(out: &mut Vec<u8>, s: &str) {
        super::str_body(out, s)
    }

    /// A `Serialize` value, by serde_json.
    pub fn serde<T: Serialize + ?Sized>(value: &T, out: &mut Vec<u8>) {
        if let Err(error) = serde_json::to_writer(&mut *out, value) {
            panic!("a value in a ToJson type could not be written as JSON: {error}");
        }
    }

    // Method resolution picks `ViaToJson` when the field's type implements ToJson (it matches
    // without an extra reference) and falls back to `ViaSerialize`, as `html!` does for Render and
    // Display.
    pub struct Field<'a, T: ?Sized>(pub &'a T);

    pub trait ViaToJson {
        fn __write(&self, out: &mut Vec<u8>);
    }

    impl<T: ToJson + ?Sized> ViaToJson for Field<'_, T> {
        #[inline]
        fn __write(&self, out: &mut Vec<u8>) {
            self.0.write_json(out)
        }
    }

    pub trait ViaSerialize {
        fn __write(&self, out: &mut Vec<u8>);
    }

    impl<T: Serialize + ?Sized> ViaSerialize for &Field<'_, T> {
        fn __write(&self, out: &mut Vec<u8>) {
            serde(self.0, out)
        }
    }
}

/// A body read by Corten's reader, or by axum (serde_json) where the reader gives up: the same
/// value, or the same rejection, as `axum::Json`.
impl<T: FromJson, S: Send + Sync> FromRequest<S> for Json<T> {
    type Rejection = JsonRejection;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        // Any other content type, `application/json` with parameters among them, is axum's to
        // judge: it parses the media type.
        if req.headers().get(header::CONTENT_TYPE).is_none_or(|ct| ct.as_bytes() != crate::api::JSON.as_bytes()) {
            let axum::Json(value) = axum::Json::<T>::from_request(req, state).await?;
            return Ok(Json(value));
        }
        let bytes = Bytes::from_request(req, state).await?;
        if let Some(value) = std::str::from_utf8(&bytes).ok().and_then(read::fast) {
            return Ok(Json(value));
        }
        let axum::Json(value) = axum::Json::<T>::from_bytes(&bytes)?;
        Ok(Json(value))
    }
}

impl<T> From<T> for Json<T> {
    fn from(value: T) -> Self {
        Json(value)
    }
}

impl<T> std::ops::Deref for Json<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> std::ops::DerefMut for Json<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};

    use super::*;
    use axum::body::to_bytes;
    use serde::Serialize;

    /// serde_json's bytes for `value`, for comparing.
    fn serde<T: Serialize + ?Sized>(value: &T) -> String {
        serde_json::to_string(value).unwrap()
    }

    #[track_caller]
    fn same<T: ToJson + Serialize + ?Sized>(value: &T) {
        assert_eq!(value.to_json(), serde(value));
    }

    #[derive(Serialize, ToJson)]
    struct Line<'a> {
        id: &'a str,
        title: String,
        quantity: i64,
        price: f64,
        rating: Option<String>,
        tags: Vec<String>,
        r#type: char,
        ok: bool,
    }

    #[derive(Serialize, ToJson)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Renamed {
        item_id: u32,
        #[serde(rename = "Weird \"key\"\n")]
        odd: (),
        #[serde(skip)]
        _hidden: u8,
        #[serde(skip_serializing, default)]
        _also_hidden: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        note: Option<&'static str>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        list: Vec<u8>,
        lines: Vec<Line<'static>>,
        shared: Box<str>,
        nested: Box<Option<Unit>>,
    }

    /// Every field can be left out, so the commas are decided as the fields are written.
    #[derive(Serialize, ToJson)]
    struct Sparse {
        #[serde(skip_serializing_if = "Option::is_none")]
        a: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        b: Option<u8>,
        c: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        d: Option<u8>,
    }

    #[derive(Serialize, ToJson)]
    struct Unit;

    #[derive(Serialize, ToJson)]
    struct Id(u64);

    #[derive(Serialize, ToJson)]
    struct Pair(&'static str, i8);

    #[derive(Serialize, ToJson)]
    #[serde(transparent)]
    struct Cents {
        value: i64,
    }

    #[derive(Serialize, ToJson)]
    struct Page<T, const N: usize> {
        rows: Vec<T>,
        size: [u8; 2],
        total: usize,
    }

    #[derive(Serialize, ToJson)]
    #[serde(rename_all = "snake_case")]
    enum Status {
        InStock,
        #[serde(rename = "gone")]
        SoldOut,
        Count(u32),
        Pair(u8, String),
        Moved {
            to: String,
            #[serde(rename = "When")]
            when: u64,
        },
    }

    #[derive(Serialize, ToJson)]
    #[serde(tag = "op", rename_all = "lowercase")]
    enum Change {
        Upsert { id: String, n: i32 },
        Delete { id: String },
        Reset,
    }

    #[derive(Serialize, ToJson)]
    #[serde(tag = "t", content = "c")]
    enum Adjacent {
        A,
        B(u8),
        C(u8, u8),
        D { x: i8 },
    }

    #[derive(Serialize, ToJson)]
    #[serde(untagged)]
    enum Either {
        Nothing,
        Number(f32),
        Two(u8, u8),
        Named { name: &'static str },
    }

    /// A type from "another crate": `Serialize` only, written by serde_json inside a derived type.
    #[derive(Serialize)]
    struct Foreign {
        at: &'static str,
    }

    #[derive(Serialize, ToJson)]
    struct WithForeign {
        when: Foreign,
        many: Vec<Foreign>,
    }

    /// A struct flattened into others: `Sparse`'s members may all be left out.
    #[derive(Serialize, ToJson)]
    #[serde(rename_all = "camelCase")]
    struct Flat {
        #[serde(flatten)]
        sparse: Sparse,
        item_id: u8,
        #[serde(flatten)]
        maybe: Option<Id2>,
        #[serde(flatten)]
        boxed: Box<Id2>,
    }

    #[derive(Serialize, ToJson)]
    struct Id2 {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<u8>,
        #[serde(rename = "Kind")]
        kind: &'static str,
    }

    /// Only flattened fields, the first of which may write nothing.
    #[derive(Serialize, ToJson)]
    struct OnlyFlat {
        #[serde(flatten)]
        empty: AllOptional,
        #[serde(flatten)]
        after: AllOptional,
    }

    #[derive(Serialize, ToJson)]
    struct AllOptional {
        #[serde(skip_serializing_if = "Option::is_none")]
        a: Option<u8>,
    }

    #[derive(Serialize, ToJson)]
    struct Empty {}

    #[derive(Serialize, ToJson)]
    struct AroundEmpty {
        #[serde(flatten)]
        empty: Empty,
        x: u8,
        #[serde(flatten)]
        again: Empty,
    }

    #[derive(Serialize, ToJson)]
    #[serde(tag = "op")]
    enum FlatChange {
        Set {
            #[serde(flatten)]
            id: Id2,
            n: i8,
        },
        Wrap {
            #[serde(flatten)]
            only: AllOptional,
        },
    }

    #[test]
    fn flattened_structs_write_what_serde_json_writes() {
        let id = |id| Id2 { id, kind: "k" };
        for (a, b, d) in [(None, None, None), (Some(1), None, Some(4)), (None, Some(2), None)] {
            for maybe in [None, Some(id(Some(9))), Some(id(None))] {
                same(&Flat { sparse: Sparse { a, b, c: 3, d }, item_id: 5, maybe, boxed: Box::new(id(None)) });
            }
        }
        for (a, b) in [(None, None), (Some(1), None), (None, Some(2)), (Some(1), Some(2))] {
            same(&OnlyFlat { empty: AllOptional { a }, after: AllOptional { a: b } });
        }
        same(&AroundEmpty { empty: Empty {}, x: 1, again: Empty {} });
        same(&FlatChange::Set { id: id(Some(1)), n: -1 });
        same(&FlatChange::Set { id: id(None), n: 0 });
        same(&FlatChange::Wrap { only: AllOptional { a: None } });
        same(&FlatChange::Wrap { only: AllOptional { a: Some(3) } });
    }

    #[test]
    fn derived_types_write_what_serde_json_writes() {
        let line = |id, title: &str| Line {
            id,
            title: title.into(),
            quantity: -3,
            price: 0.1 + 0.2,
            rating: None,
            tags: vec!["a".into(), "<b>".into()],
            r#type: 'é',
            ok: true,
        };
        same(&line("l1", "Plain"));
        same(&Renamed {
            item_id: 7,
            odd: (),
            _hidden: 1,
            _also_hidden: 2,
            note: None,
            list: vec![],
            lines: vec![line("l1", "Q \"quoted\" \\ back\tslash \u{1} \u{1f} \u{7f} 日本 🎴"), line("", "")],
            shared: "x".into(),
            nested: Box::new(Some(Unit)),
        });
        same(&Renamed {
            item_id: 0,
            odd: (),
            _hidden: 0,
            _also_hidden: 0,
            note: Some("n"),
            list: vec![1, 2],
            lines: vec![],
            shared: "".into(),
            nested: Box::new(None),
        });
        for (a, b, d) in
            [(None, None, None), (Some(1), None, None), (None, Some(2), None), (None, None, Some(4)), (Some(1), Some(2), Some(4))]
        {
            same(&Sparse { a, b, c: 3, d });
        }
        same(&Unit);
        same(&Id(u64::MAX));
        same(&Pair("p", i8::MIN));
        same(&Cents { value: -150 });
        same(&Page::<_, 3> { rows: vec![Id(1), Id(2)], size: [1, 2], total: 2 });
        same(&Page::<u8, 0> { rows: vec![], size: [0, 0], total: 0 });
        for status in [
            Status::InStock,
            Status::SoldOut,
            Status::Count(3),
            Status::Pair(1, "x".into()),
            Status::Moved { to: "y".into(), when: 9 },
        ] {
            same(&status);
        }
        for change in [Change::Upsert { id: "a".into(), n: -1 }, Change::Delete { id: "b".into() }, Change::Reset] {
            same(&change);
        }
        for adjacent in [Adjacent::A, Adjacent::B(1), Adjacent::C(1, 2), Adjacent::D { x: -1 }] {
            same(&adjacent);
        }
        for either in [Either::Nothing, Either::Number(1.5), Either::Two(1, 2), Either::Named { name: "n" }] {
            same(&either);
        }
        same(&WithForeign { when: Foreign { at: "2026-10-24" }, many: vec![Foreign { at: "a" }, Foreign { at: "b" }] });
    }

    #[test]
    fn std_types_write_what_serde_json_writes() {
        let text: String = (0u8..0x80).map(char::from).chain("é日🎴\u{2028}".chars()).collect();
        for len in 0..text.len() {
            if text.is_char_boundary(len) {
                same(&text[..len]);
            }
        }
        for n in [0.0, -0.0, 1.0, 0.1, 1e21, 1e-7, 123456789.125, f64::MAX, f64::MIN_POSITIVE, 5e-324, f64::NAN, f64::INFINITY] {
            same(&n);
            same(&(n as f32));
        }
        for n in [0, 1, 9, 10, 99, 100, 12_345, i64::MAX, i64::MIN] {
            same(&n);
            same(&(n as u64));
            same(&(n as i8));
            same(&(n as u128));
            same(&(n as i128 * 1_000_000_000_000));
        }
        same(&(1u8, "two", [3.5f64], ((),), vec![Some(true), None]));
        let mut by_id = HashMap::new();
        by_id.insert(7u32, "seven");
        by_id.insert(70, "seventy");
        same(&by_id);
        let mut flags = BTreeMap::new();
        flags.insert(true, 'y');
        flags.insert(false, 'n');
        same(&flags);
        let mut names: BTreeMap<String, Vec<i16>> = BTreeMap::new();
        names.insert("a\"b".into(), vec![-1, 2]);
        same(&names);
        same(&serde_json::json!({ "id": "item_1", "price": 0.5, "tags": ["a", "<b>"], "none": null }));
    }

    #[tokio::test]
    async fn writes_what_axum_writes() {
        let value = serde_json::json!({ "id": "item_1", "price": 0.5, "tags": ["a", "<b>"], "none": null });
        let ours = Json(&value).into_response();
        let theirs = axum::Json(&value).into_response();
        assert_eq!(ours.status(), theirs.status());
        assert_eq!(ours.headers()[header::CONTENT_TYPE], theirs.headers()[header::CONTENT_TYPE]);
        assert_eq!(axum::body::HttpBody::size_hint(ours.body()).exact(), axum::body::HttpBody::size_hint(theirs.body()).exact());
        let (ours, theirs) =
            (to_bytes(ours.into_body(), usize::MAX).await.unwrap(), to_bytes(theirs.into_body(), usize::MAX).await.unwrap());
        assert_eq!(ours, theirs);
    }

    #[test]
    fn a_derived_type_sizes_its_next_vector_from_its_last() {
        let first = to_vec(&Id(12_345));
        assert_eq!(first, b"12345");
        let hint = Id::__json_size().unwrap().load(Ordering::Relaxed);
        assert_eq!(hint, 5);
        assert_eq!(<Arc<Id>>::__json_size().map(|s| s as *const _), Id::__json_size().map(|s| s as *const _));
    }

    #[derive(Debug, PartialEq, serde::Deserialize, FromJson)]
    struct Alert {
        email: String,
        target: Option<u32>,
    }

    /// The extractor's answer for one request: the value, or the rejection's status and text.
    async fn read<T: FromJson + std::fmt::Debug>(ct: Option<&str>, body: &'static [u8]) -> (String, String) {
        let mut req = Request::post("/");
        if let Some(ct) = ct {
            req = req.header(header::CONTENT_TYPE, ct);
        }
        let ours = Json::<T>::from_request(req.body(Body::from(body)).unwrap(), &()).await;
        match ours {
            Ok(Json(v)) => ("ok".into(), format!("{v:?}")),
            Err(e) => (e.status().to_string(), e.body_text()),
        }
    }

    async fn axum_reads<T: serde::de::DeserializeOwned + std::fmt::Debug>(
        ct: Option<&str>,
        body: &'static [u8],
    ) -> (String, String) {
        let mut req = Request::post("/");
        if let Some(ct) = ct {
            req = req.header(header::CONTENT_TYPE, ct);
        }
        match axum::Json::<T>::from_request(req.body(Body::from(body)).unwrap(), &()).await {
            Ok(axum::Json(v)) => ("ok".into(), format!("{v:?}")),
            Err(e) => (e.status().to_string(), e.body_text()),
        }
    }

    #[tokio::test]
    async fn reads_a_body_as_axum_does() {
        let types = [
            None,
            Some("application/json"),
            Some("application/json; charset=utf-8"),
            Some("APPLICATION/JSON"),
            Some("application/problem+json"),
            Some("application/json;;"),
            Some("text/plain"),
            Some("application/jsonx"),
        ];
        let bodies: [&[u8]; 12] = [
            br#"{"email":"a@b.c","target":5}"#,
            br#"{"target":null,"email":"a@b.c","extra":[1,{"x":2}]}"#,
            br#"{"email":"a@b.c"}"#,
            br#" {"email" : "a\u0040b.c" , "target" : 7 } "#,
            br#"{"target":5}"#,
            br#"{"email":"a@b.c","target":-1}"#,
            br#"{"email":"a@b.c","target":5,"email":"x"}"#,
            br#"{"email":"a@b.c""#,
            br#"{"email":"a@b.c"} x"#,
            br#"["a@b.c",5]"#,
            b"{\"email\":\"\xff\"}",
            b"",
        ];
        for ct in types {
            for body in bodies {
                let text = String::from_utf8_lossy(body);
                assert_eq!(read::<Alert>(ct, body).await, axum_reads::<Alert>(ct, body).await, "{ct:?} {text}");
                assert_eq!(
                    read::<serde_json::Value>(ct, body).await,
                    axum_reads::<serde_json::Value>(ct, body).await,
                    "{ct:?} {text}"
                );
            }
        }
        assert_eq!(read::<Alert>(Some("application/json"), bodies[0]).await.0, "ok");
    }
}
