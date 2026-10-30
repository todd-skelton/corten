//! Reading `application/x-www-form-urlencoded` into a struct, the way a browser posts a form:
//! a key sent once per value. Pairs are grouped by key (keeping the order the values came in),
//! so a `Vec` field gets every value of its key and any other field exactly one.
//!
//! | Field | Reads |
//! |---|---|
//! | `String`, a number, a unit-variant enum | the key's one value; sent twice is an error |
//! | `bool` | `true` for `true` or `on` (a checked checkbox), `false` for `false` |
//! | `Option<T>` | `None` when the key is missing or its value is empty (a blank field), else `T` |
//! | `Vec<T>` | every value of the key, in order |
//!
//! A key the browser leaves out (an unchecked checkbox, a list with nothing selected) is
//! missing, as serde sees it: `#[derive(Fields)]` requires `#[serde(default)]` on a `bool` or a
//! list so that reads as `false` or empty instead of failing.

use std::{borrow::Cow, fmt};

use serde::de::{self, value::StrDeserializer, DeserializeSeed, Deserializer, IntoDeserializer, MapAccess, SeqAccess, Visitor};

pub type Pair<'a> = (Cow<'a, str>, Cow<'a, str>);

/// Why a form or query string did not read into its type: the field and what was wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormError(String);

impl fmt::Display for FormError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FormError {}

impl de::Error for FormError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        FormError(msg.to_string())
    }
}

/// Reads `bytes` (a body or a query string, without the `?`) into `T`.
pub fn from_bytes<T: de::DeserializeOwned>(bytes: &[u8]) -> Result<T, FormError> {
    from_pairs(form_urlencoded::parse(bytes).collect())
}

/// Reads the fields a form sent, in the order it sent them, into `T`.
pub(crate) fn from_pairs<T: de::DeserializeOwned>(mut pairs: Vec<Pair<'_>>) -> Result<T, FormError> {
    // Stable: the values of one key stay in the order they were sent.
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    T::deserialize(Form(&pairs))
}

/// A query string's pairs sorted by key (the values of one key in the order sent), for
/// [`field`] and [`prefixed`]: what `Params` reads a route's query parameters from.
pub fn sorted_pairs(query: &str) -> Vec<Pair<'_>> {
    let mut pairs: Vec<Pair<'_>> = form_urlencoded::parse(query.as_bytes()).collect();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    pairs
}

/// One field of a route's query, by its key, from [`sorted_pairs`]. A key that is missing is
/// `None` for an `Option`, empty for a list, and `default` (`#[serde(default)]`) or an
/// error for anything else, the way a link leaves those out.
pub fn field<T: de::DeserializeOwned>(pairs: &[Pair<'_>], key: &str, default: Option<fn() -> T>) -> Result<T, FormError> {
    let start = pairs.partition_point(|(k, _)| k.as_ref() < key);
    let end = start + pairs[start..].iter().take_while(|(k, _)| k == key).count();
    match (&pairs[start..end], default) {
        ([], Some(default)) => Ok(default()),
        ([], None) => T::deserialize(Missing(key)),
        (values, _) => T::deserialize(Values(values)).map_err(|e| FormError(format!("{key}: {e}"))),
    }
}

/// A `#[route(prefix = "dim.")]` field's parameters: each whose key starts with `prefix`, the
/// prefix taken off, in order.
pub fn prefixed<'b>(pairs: &'b [Pair<'_>], prefix: &'b str) -> impl Iterator<Item = (&'b str, &'b str)> {
    let start = pairs.partition_point(|(k, _)| k.as_ref() < prefix);
    pairs[start..].iter().map_while(move |(k, v)| Some((k.strip_prefix(prefix)?, v.as_ref())))
}

/// One text read into `T` as a form field's value is.
pub fn one<T: de::DeserializeOwned>(text: &str) -> Result<T, FormError> {
    T::deserialize(Values(&[(Cow::Borrowed(""), Cow::Borrowed(text))]))
}

/// A key the query does not have: `None`, an empty list, or the error serde gives a missing field.
struct Missing<'b>(&'b str);

impl<'de> Deserializer<'de> for Missing<'_> {
    type Error = FormError;

    fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, FormError> {
        Err(FormError(format!("missing field `{}`", self.0)))
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, FormError> {
        visitor.visit_none()
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, FormError> {
        visitor.visit_seq(List([].iter()))
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf
        unit unit_struct newtype_struct tuple tuple_struct map struct enum identifier ignored_any
    }
}

/// The whole form: a map from each key to its values.
struct Form<'a, 'b>(&'b [Pair<'a>]);

impl<'de> Deserializer<'de> for Form<'_, '_> {
    type Error = FormError;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, FormError> {
        visitor.visit_map(Groups::new(self.0, 0, None))
    }

    /// A struct's fields, and its groups and rows (`address.street`, `domains[key].max`).
    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, FormError> {
        visitor.visit_map(Groups::new(self.0, 0, Some(fields)))
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(self, _: &'static str, visitor: V) -> Result<V::Value, FormError> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, FormError> {
        visitor.visit_unit()
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf option
        unit_struct seq tuple tuple_struct map enum identifier ignored_any
    }
}

/// The form's keys (or a group's, each past `skip` bytes of its prefix), each with every value
/// sent for it. Reading a struct (`fields`), a key that is not one of its fields but starts with
/// one and `.` or `[` is that field's group or rows, with every key that shares the prefix.
struct Groups<'a, 'b> {
    rest: &'b [Pair<'a>],
    skip: usize,
    fields: Option<&'static [&'static str]>,
    values: &'b [Pair<'a>],
    /// The current key's group (`.`) or rows (`[`), and its prefix's length.
    nested: Option<(u8, usize)>,
    /// Every key of this map, sorted: a field sent as `x` is a value of its own, not a group, so
    /// `x.y` keys beside it (an image button's `x.x`) are left to serde, as on any form.
    all: &'b [Pair<'a>],
}

impl<'a, 'b> Groups<'a, 'b> {
    fn new(rest: &'b [Pair<'a>], skip: usize, fields: Option<&'static [&'static str]>) -> Self {
        Groups { rest, skip, fields, values: &[], nested: None, all: rest }
    }

    /// Whether the form sent `key` itself, as a value.
    fn sent(&self, key: &str) -> bool {
        self.all.binary_search_by(|(k, _)| k.as_ref().cmp(key)).is_ok()
    }
}

impl<'de> MapAccess<'de> for Groups<'_, '_> {
    type Error = FormError;

    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>, FormError> {
        let Some((first, _)) = self.rest.first() else { return Ok(None) };
        let local = &first[self.skip..];
        let split = self.fields.and_then(|fields| {
            // Bytes, not chars: `.` and `[` are ASCII, and most keys have neither.
            let at = local.bytes().position(|b| b == b'.' || b == b'[')?;
            let head = &local[..at];
            let split = !fields.contains(&local) && fields.contains(&head);
            (split && !self.sent(&first[..self.skip + at])).then_some(at)
        });
        let (count, key) = match split {
            None => {
                self.nested = None;
                (self.rest.iter().take_while(|(k, _)| k == first).count(), local)
            }
            Some(at) => {
                let prefix = &first[..self.skip + at + 1];
                self.nested = Some((local.as_bytes()[at], prefix.len()));
                (self.rest.iter().take_while(|(k, _)| k.starts_with(prefix)).count(), &local[..at])
            }
        };
        let (values, rest) = self.rest.split_at(count);
        (self.values, self.rest) = (values, rest);
        let key: StrDeserializer<'_, FormError> = key.into_deserializer();
        seed.deserialize(key).map(Some)
    }

    #[inline]
    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, FormError> {
        let Some((sep, skip)) = self.nested else {
            let values = self.values;
            return seed.deserialize(Values(values)).map_err(|e| FormError(format!("{}: {e}", values[0].0)));
        };
        nested_value(self.values, sep, skip, seed)
    }

    fn size_hint(&self) -> Option<usize> {
        None
    }
}

/// A group's or rows' value, out of line so a plain field's read stays small enough to inline.
#[inline(never)]
fn nested_value<'de, V: DeserializeSeed<'de>>(pairs: &[Pair<'_>], sep: u8, skip: usize, seed: V) -> Result<V::Value, FormError> {
    let full = pairs.first().map_or("", |(k, _)| k.as_ref());
    seed.deserialize(Nested { pairs, skip, rows: sep == b'[', path: &full[..skip - 1] })
}

/// A group's fields (`address.street`, `rows` false) or rows' (`domains[key].max`): the keys
/// that start with `path` and its separator, `skip` bytes long.
struct Nested<'a, 'b> {
    pairs: &'b [Pair<'a>],
    skip: usize,
    rows: bool,
    path: &'b str,
}

impl Nested<'_, '_> {
    /// serde's own errors ("missing field", "unknown field") name the field alone: the group's
    /// path goes in front, unless a value's error already starts with its full key.
    fn at(&self, error: FormError) -> FormError {
        if error.0.starts_with(self.path) {
            error
        } else {
            FormError(format!("{}: {}", self.path, error.0))
        }
    }
}

impl<'de> Deserializer<'de> for Nested<'_, '_> {
    type Error = FormError;

    /// A value that is not a group or rows (`go: bool` with only an image button's `go.x` sent):
    /// keys under its name are read as a group's or rows', so it was not sent as itself.
    fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, FormError> {
        Err(FormError(format!(
            "{0}: sent only as {0}. or {0}[ keys, which are a group's or rows', and not as {0} itself",
            self.path
        )))
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, FormError> {
        let result = if self.rows {
            visitor.visit_map(Rows { rest: self.pairs, skip: self.skip, values: &[], row: 0 })
        } else {
            visitor.visit_map(Groups::new(self.pairs, self.skip, None))
        };
        result.map_err(|e| self.at(e))
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, FormError> {
        if self.rows {
            return Err(FormError(format!("{}: rows are a map from each row's key to its fields", self.path)));
        }
        visitor.visit_map(Groups::new(self.pairs, self.skip, Some(fields))).map_err(|e| self.at(e))
    }

    /// A group or rows is never optional (`#[derive(Fields)]` refuses it), so an `Option` field
    /// whose own key was not sent is `None` (an image button's `go.x` beside `go: Option<_>`).
    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, FormError> {
        visitor.visit_none()
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(self, _: &'static str, visitor: V) -> Result<V::Value, FormError> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, FormError> {
        visitor.visit_unit()
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf
        unit unit_struct seq tuple tuple_struct enum identifier
    }
}

/// Rows' keys, each with the fields of its row: `domains[key].max` past `domains[`.
struct Rows<'a, 'b> {
    rest: &'b [Pair<'a>],
    skip: usize,
    values: &'b [Pair<'a>],
    /// The length of the current row's prefix, `domains[key].`.
    row: usize,
}

impl<'de> MapAccess<'de> for Rows<'_, '_> {
    type Error = FormError;

    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>, FormError> {
        let Some((first, _)) = self.rest.first() else { return Ok(None) };
        let local = &first[self.skip..];
        // `rows[key].field` (a row's field), or `map[key]` alone (a map's value).
        let close = local
            .find(']')
            .filter(|close| matches!(local.as_bytes().get(close + 1), Some(b'.') | None))
            .ok_or_else(|| FormError(format!("{first}: a row's field is named rows[key].field, a map's value map[key]")))?;
        let value = close + 1 == local.len();
        self.row = self.skip + close + if value { 1 } else { 2 };
        let prefix = &first[..self.row];
        let count = if value {
            self.rest.iter().take_while(|(k, _)| k == first).count()
        } else {
            self.rest.iter().take_while(|(k, _)| k.starts_with(prefix)).count()
        };
        let (values, rest) = self.rest.split_at(count);
        (self.values, self.rest) = (values, rest);
        let key = decode_key(&local[..close])
            .ok_or_else(|| FormError(format!("{first}: the row's key is not written the way the form writes it")))?;
        let pair: Pair<'_> = (Cow::Borrowed(""), key);
        seed.deserialize(RowKeyText(std::slice::from_ref(&pair)))
            .map(Some)
            .map_err(|e| FormError(format!("{}: key: {e}", &prefix[..prefix.len() - 1])))
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, FormError> {
        let full = self.values.first().map_or("", |(k, _)| k.as_ref());
        if self.row == full.len() {
            // A map's value: the key's own values.
            return seed.deserialize(Values(self.values)).map_err(|e| FormError(format!("{full}: {e}")));
        }
        seed.deserialize(Nested { pairs: self.values, skip: self.row, rows: false, path: &full[..self.row - 1] })
    }

    fn size_hint(&self) -> Option<usize> {
        None
    }
}

/// A row's key, read as its type (text, an integer, an enum's word). An integer is read only as
/// it writes itself (not `07`, `+7` or `-0`), so one integer key cannot reach a row two ways.
struct RowKeyText<'a, 'b>(&'b [Pair<'a>]);

macro_rules! strict_integers {
    ($($method:ident => $visit:ident: $t:ty,)*) => {$(
        fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, FormError> {
            let text = Values(self.0).one()?;
            match text.parse::<$t>() {
                Ok(value) if value.to_string() == text => visitor.$visit(value),
                _ => Err(de::Error::invalid_value(de::Unexpected::Str(text), &"an integer as the form writes it")),
            }
        }
    )*};
}

macro_rules! as_values {
    ($($method:ident,)*) => {$(
        fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, FormError> {
            Values(self.0).$method(visitor)
        }
    )*};
}

impl<'de> Deserializer<'de> for RowKeyText<'_, '_> {
    type Error = FormError;

    strict_integers! {
        deserialize_i8 => visit_i8: i8,
        deserialize_i16 => visit_i16: i16,
        deserialize_i32 => visit_i32: i32,
        deserialize_i64 => visit_i64: i64,
        deserialize_i128 => visit_i128: i128,
        deserialize_u8 => visit_u8: u8,
        deserialize_u16 => visit_u16: u16,
        deserialize_u32 => visit_u32: u32,
        deserialize_u64 => visit_u64: u64,
        deserialize_u128 => visit_u128: u128,
    }

    as_values! {
        deserialize_any, deserialize_bool, deserialize_f32, deserialize_f64, deserialize_char, deserialize_str,
        deserialize_string, deserialize_bytes, deserialize_byte_buf, deserialize_option, deserialize_unit,
        deserialize_seq, deserialize_map, deserialize_identifier, deserialize_ignored_any,
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(self, name: &'static str, visitor: V) -> Result<V::Value, FormError> {
        Values(self.0).deserialize_unit_struct(name, visitor)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(self, _: &'static str, visitor: V) -> Result<V::Value, FormError> {
        // A newtype key (`struct Sku(u32)`) is read as the value it wraps, strictly too.
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_tuple<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value, FormError> {
        Values(self.0).deserialize_tuple(len, visitor)
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, FormError> {
        Values(self.0).deserialize_tuple_struct(name, len, visitor)
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, FormError> {
        Values(self.0).deserialize_struct(name, fields, visitor)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, FormError> {
        Values(self.0).deserialize_enum(name, variants, visitor)
    }
}

/// A row's key as `FieldName` writes it, its `%XX` decoded; `None` for a key written any other
/// way (`%61` for `a`, lowercase hex, a character the form encodes), so one key cannot be sent
/// two ways and land in one row.
fn decode_key(text: &str) -> Option<Cow<'_, str>> {
    let plain = |b: u8| b.is_ascii_alphanumeric() || b == b'-' || b == b'_';
    let hex = |b: u8| match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    };
    let bytes = text.as_bytes();
    if bytes.iter().all(|&b| plain(b)) {
        return Some(Cow::Borrowed(text));
    }
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let byte = hex(*bytes.get(i + 1)?)? << 4 | hex(*bytes.get(i + 2)?)?;
            if plain(byte) {
                return None;
            }
            out.push(byte);
            i += 3;
        } else if plain(bytes[i]) {
            out.push(bytes[i]);
            i += 1;
        } else {
            return None;
        }
    }
    String::from_utf8(out).ok().map(Cow::Owned)
}

/// The values of one key: a list for a sequence, else exactly one.
struct Values<'a, 'b>(&'b [Pair<'a>]);

impl<'b> Values<'_, 'b> {
    fn one(&self) -> Result<&'b str, FormError> {
        match self.0 {
            [(_, value)] => Ok(value),
            [] => Err(FormError("no value".into())),
            _ => Err(FormError(format!("sent {} times; a field that takes several values is a Vec", self.0.len()))),
        }
    }
}

macro_rules! parse {
    ($($method:ident => $visit:ident,)*) => {$(
        fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, FormError> {
            let text = self.one()?;
            match text.parse() {
                Ok(value) => visitor.$visit(value),
                Err(_) => Err(de::Error::invalid_value(de::Unexpected::Str(text), &visitor)),
            }
        }
    )*};
}

impl<'de> Deserializer<'de> for Values<'_, '_> {
    type Error = FormError;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, FormError> {
        match self.0 {
            [(_, value)] => visitor.visit_str(value),
            values => visitor.visit_seq(List(values.iter())),
        }
    }

    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, FormError> {
        match self.one()? {
            "true" | "on" => visitor.visit_bool(true),
            "false" => visitor.visit_bool(false),
            other => Err(de::Error::invalid_value(de::Unexpected::Str(other), &"true, on or false")),
        }
    }

    parse! {
        deserialize_i8 => visit_i8,
        deserialize_i16 => visit_i16,
        deserialize_i32 => visit_i32,
        deserialize_i64 => visit_i64,
        deserialize_i128 => visit_i128,
        deserialize_u8 => visit_u8,
        deserialize_u16 => visit_u16,
        deserialize_u32 => visit_u32,
        deserialize_u64 => visit_u64,
        deserialize_u128 => visit_u128,
        deserialize_f32 => visit_f32,
        deserialize_f64 => visit_f64,
        deserialize_char => visit_char,
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, FormError> {
        visitor.visit_str(self.one()?)
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, FormError> {
        visitor.visit_str(self.one()?)
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, FormError> {
        visitor.visit_str(self.one()?)
    }

    fn deserialize_bytes<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, FormError> {
        visitor.visit_bytes(self.one()?.as_bytes())
    }

    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, FormError> {
        visitor.visit_bytes(self.one()?.as_bytes())
    }

    /// A blank field is no value: `<input name="max">` left empty is `None`, not `Some("")`.
    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, FormError> {
        match self.0 {
            [(_, value)] if value.is_empty() => visitor.visit_none(),
            _ => visitor.visit_some(self),
        }
    }

    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, FormError> {
        visitor.visit_unit()
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(self, _: &'static str, visitor: V) -> Result<V::Value, FormError> {
        visitor.visit_unit()
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(self, _: &'static str, visitor: V) -> Result<V::Value, FormError> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, FormError> {
        visitor.visit_seq(List(self.0.iter()))
    }

    fn deserialize_tuple<V: Visitor<'de>>(self, _: usize, visitor: V) -> Result<V::Value, FormError> {
        visitor.visit_seq(List(self.0.iter()))
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(self, _: &'static str, _: usize, visitor: V) -> Result<V::Value, FormError> {
        visitor.visit_seq(List(self.0.iter()))
    }

    fn deserialize_map<V: Visitor<'de>>(self, _: V) -> Result<V::Value, FormError> {
        Err(FormError("a form field is text, not a map; name each field in the form struct".into()))
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        _: V,
    ) -> Result<V::Value, FormError> {
        Err(FormError("a form field is text, not a struct; name each field in the form struct".into()))
    }

    /// A unit variant, by its name: `<select name="sort">` into `enum Sort { Price, New }`.
    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, FormError> {
        let variant: StrDeserializer<'_, FormError> = self.one()?.into_deserializer();
        visitor.visit_enum(variant)
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, FormError> {
        visitor.visit_unit()
    }
}

/// A key's values, one element each.
struct List<'a, 'b>(std::slice::Iter<'b, Pair<'a>>);

impl<'de> SeqAccess<'de> for List<'_, '_> {
    type Error = FormError;

    fn next_element_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Value>, FormError> {
        match self.0.next() {
            Some(pair) => seed.deserialize(Values(std::slice::from_ref(pair))).map(Some),
            None => Ok(None),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.0.len())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde::Deserialize;

    use super::*;

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(rename_all = "lowercase")]
    enum Sort {
        Price,
        New,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct Bulk {
        #[serde(default)]
        ids: Vec<String>,
        #[serde(default)]
        counts: Vec<u32>,
        #[serde(default)]
        notify: bool,
        max: Option<u32>,
        note: Option<String>,
        sort: Sort,
    }

    #[test]
    fn repeated_keys_are_a_list_in_the_order_sent() {
        let form: Bulk =
            from_bytes(b"ids=b&sort=price&ids=a&counts=2&ids=c%20d&counts=1&notify=on&max=5&note=hi+there").expect("reads");
        assert_eq!(form.ids, ["b", "a", "c d"]);
        assert_eq!(form.counts, [2, 1]);
        assert!(form.notify);
        assert_eq!(form.max, Some(5));
        assert_eq!(form.note.as_deref(), Some("hi there"));
        assert_eq!(form.sort, Sort::Price);
    }

    #[test]
    fn what_a_browser_leaves_out_or_leaves_blank() {
        // No box checked, the checkbox unchecked, the number field blank, the text field missing.
        let form: Bulk = from_bytes(b"max=&sort=new").expect("reads");
        assert_eq!(form, Bulk { ids: vec![], counts: vec![], notify: false, max: None, note: None, sort: Sort::New });
        let one: Bulk = from_bytes(b"ids=only&sort=new&notify=false").expect("reads");
        assert_eq!(one.ids, ["only"]);
        assert!(!one.notify);
    }

    #[test]
    fn mistakes_name_the_field() {
        let err = |body: &[u8]| from_bytes::<Bulk>(body).unwrap_err().to_string();
        assert_eq!(err(b"sort=new&sort=price"), "sort: sent 2 times; a field that takes several values is a Vec");
        assert!(
            err(b"sort=new&counts=1&counts=x").starts_with("counts: invalid value: string \"x\""),
            "{}",
            err(b"sort=new&counts=x")
        );
        assert!(err(b"sort=old").starts_with("sort: unknown variant `old`"), "{}", err(b"sort=old"));
        assert!(err(b"sort=new&notify=yes").starts_with("notify: invalid value: string \"yes\""));
        assert_eq!(err(b"ids=a"), "missing field `sort`");
    }

    #[test]
    fn maps_and_unknown_keys() {
        let map: BTreeMap<String, String> = from_bytes(b"b=2&a=1").expect("reads");
        assert_eq!(map, BTreeMap::from([("a".into(), "1".into()), ("b".into(), "2".into())]));
        let lists: BTreeMap<String, Vec<String>> = from_bytes(b"a=1&a=2").expect("reads");
        assert_eq!(lists["a"], ["1", "2"]);
        // A key the struct does not name is ignored, as serde does by default.
        let form: Bulk = from_bytes(b"sort=new&utm_source=x&utm_source=y").expect("reads");
        assert_eq!(form.sort, Sort::New);
        let (): () = from_bytes(b"").expect("an empty form is unit");
    }
}
