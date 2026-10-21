//! Message catalogs and locale negotiation for server-rendered text.
//!
//! A [`Catalog`] maps keys to messages with `{token}` placeholders. A missing key renders as
//! `[missing:{locale}:{key}]` so it is visible on the page and easy to find in a test, and
//! [`Catalog::missing_from`] checks one catalog against another in CI. Pick the catalog per request
//! with [`Locales::negotiate`] and pass it to the template:
//!
//! ```ignore
//! <h2>{{ t.t("item.listings") }}</h2>
//! <p>{{ t.msg("item.available").arg("count", item.quantity) }}</p>
//! ```
//!
//! Numbers, money and dates are formatted by the app into its view models (README rule 7), with
//! the negotiated locale: `catalog.locale()` is a BCP 47 tag to hand to a CLDR library such as
//! ICU4X (`icu` crate). Corten carries no locale data, because correct formatting for every
//! locale needs all of it, and a partial table prints wrong numbers without an error.

use std::{borrow::Cow, collections::HashMap, fmt::Display};

/// Messages for one locale.
#[derive(Clone, Debug)]
pub struct Catalog {
    locale: String,
    messages: HashMap<String, String>,
}

impl Catalog {
    pub fn new<K: Into<String>, V: Into<String>>(locale: &str, messages: impl IntoIterator<Item = (K, V)>) -> Self {
        Catalog { locale: locale.into(), messages: messages.into_iter().map(|(k, v)| (k.into(), v.into())).collect() }
    }

    /// Reads nested JSON objects of strings; keys are joined with `.` (`{"item":{"title":".."}}`
    /// becomes `item.title`).
    #[cfg(feature = "json")]
    pub fn from_json(locale: &str, json: &str) -> Result<Self, serde_json::Error> {
        fn walk(prefix: &str, value: &serde_json::Value, out: &mut HashMap<String, String>) {
            match value {
                serde_json::Value::Object(map) => {
                    for (k, v) in map {
                        let key = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
                        walk(&key, v, out);
                    }
                }
                serde_json::Value::String(s) => {
                    out.insert(prefix.to_string(), s.clone());
                }
                other => {
                    out.insert(prefix.to_string(), other.to_string());
                }
            }
        }
        let value: serde_json::Value = serde_json::from_str(json)?;
        let mut messages = HashMap::new();
        walk("", &value, &mut messages);
        Ok(Catalog { locale: locale.into(), messages })
    }

    pub fn locale(&self) -> &str {
        &self.locale
    }

    /// The message for `key`, or `[missing:{locale}:{key}]`.
    pub fn t(&self, key: &str) -> Cow<'_, str> {
        match self.messages.get(key) {
            Some(message) => Cow::Borrowed(message),
            None => Cow::Owned(format!("[missing:{}:{key}]", self.locale)),
        }
    }

    /// The message for `key` with each `{name}` replaced. Unknown placeholders stay as written.
    pub fn tf(&self, key: &str, args: &[(&str, &dyn Display)]) -> String {
        interpolate(&self.t(key), |name| args.iter().find(|(n, _)| *n == name).map(|(_, v)| v.to_string()))
    }

    /// A message to fill in with [`Message::arg`]; prints itself. Reads well in templates:
    /// `{{ t.msg("item.available").arg("count", n) }}`.
    pub fn msg<'a>(&'a self, key: &'a str) -> Message<'a> {
        Message { catalog: self, key, args: Vec::new() }
    }

    /// Keys `reference` has that this catalog lacks, sorted. Use it in a test so a new key cannot
    /// ship untranslated.
    pub fn missing_from(&self, reference: &Catalog) -> Vec<String> {
        let mut keys: Vec<String> = reference.messages.keys().filter(|k| !self.messages.contains_key(*k)).cloned().collect();
        keys.sort();
        keys
    }
}

/// A catalog message with its arguments, from [`Catalog::msg`]. Printing it interpolates.
pub struct Message<'a> {
    catalog: &'a Catalog,
    key: &'a str,
    args: Vec<(&'a str, String)>,
}

impl<'a> Message<'a> {
    pub fn arg(mut self, name: &'a str, value: impl Display) -> Self {
        self.args.push((name, value.to_string()));
        self
    }
}

impl Display for Message<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text =
            interpolate(&self.catalog.t(self.key), |name| self.args.iter().find(|(n, _)| *n == name).map(|(_, v)| v.clone()));
        f.write_str(&text)
    }
}

fn interpolate(message: &str, lookup: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(message.len() + 16);
    let mut rest = message;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) => {
                let name = &after[..close];
                match lookup(name) {
                    Some(value) => out.push_str(&value),
                    None => {
                        out.push('{');
                        out.push_str(name);
                        out.push('}');
                    }
                }
                rest = &after[close + 1..];
            }
            None => {
                out.push_str(&rest[open..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Every catalog the app ships; the first is the default.
#[derive(Clone, Debug)]
pub struct Locales {
    catalogs: Vec<Catalog>,
}

impl Locales {
    pub fn new(default: Catalog) -> Self {
        Locales { catalogs: vec![default] }
    }

    pub fn with(mut self, catalog: Catalog) -> Self {
        self.catalogs.push(catalog);
        self
    }

    pub fn default_catalog(&self) -> &Catalog {
        &self.catalogs[0]
    }

    pub fn get(&self, locale: &str) -> Option<&Catalog> {
        self.catalogs.iter().find(|c| c.locale.eq_ignore_ascii_case(locale))
    }

    /// The best catalog for an `Accept-Language` header: highest `q` first, exact tag before the
    /// primary language (`fr-CA` matches `fr`), then the default.
    pub fn negotiate(&self, accept_language: Option<&str>) -> &Catalog {
        let mut wanted: Vec<(&str, f32)> = accept_language
            .unwrap_or("")
            .split(',')
            .filter_map(|part| {
                let mut pieces = part.trim().split(';');
                let tag = pieces.next()?.trim();
                let q = pieces.find_map(|p| p.trim().strip_prefix("q=")).map_or(Some(1.0), |q| q.parse().ok())?;
                (!tag.is_empty() && tag != "*" && q > 0.0).then_some((tag, q))
            })
            .collect();
        wanted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        for (tag, _) in wanted {
            if let Some(catalog) = self.get(tag) {
                return catalog;
            }
            let primary = tag.split('-').next().unwrap_or(tag);
            if let Some(catalog) =
                self.catalogs.iter().find(|c| c.locale.split('-').next().unwrap_or("").eq_ignore_ascii_case(primary))
            {
                return catalog;
            }
        }
        self.default_catalog()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_interpolate_and_missing_keys_are_visible() {
        let en = Catalog::new("en", [("item.available", "{count} available from {sellers} sellers"), ("a", "A {x")]);
        assert_eq!(en.tf("item.available", &[("count", &3), ("sellers", &"two")]), "3 available from two sellers");
        assert_eq!(en.tf("item.available", &[("count", &3)]), "3 available from {sellers} sellers");
        assert_eq!(en.tf("a", &[("x", &1)]), "A {x", "an unclosed brace is printed as written");
        assert_eq!(en.msg("item.available").arg("count", 3).arg("sellers", "two").to_string(), "3 available from two sellers");
        assert_eq!(en.t("nope"), "[missing:en:nope]");
        let fr = Catalog::new("fr", [("a", "A")]);
        assert_eq!(fr.missing_from(&en), vec!["item.available".to_string()]);
    }

    #[cfg(feature = "json")]
    #[test]
    fn json_catalogs_flatten_nested_keys() {
        let c = Catalog::from_json("en", r#"{"item":{"title":"Item","count":{"one":"1 item"}}}"#).unwrap();
        assert_eq!(c.t("item.title"), "Item");
        assert_eq!(c.t("item.count.one"), "1 item");
    }

    #[test]
    fn negotiation_prefers_quality_then_primary_language() {
        let locales = Locales::new(Catalog::new::<&str, &str>("en", []))
            .with(Catalog::new::<&str, &str>("fr", []))
            .with(Catalog::new::<&str, &str>("pt-BR", []));
        assert_eq!(locales.negotiate(Some("fr-CA,fr;q=0.9,en;q=0.8")).locale(), "fr");
        assert_eq!(locales.negotiate(Some("de, pt-br;q=0.5, en;q=0.4")).locale(), "pt-BR");
        assert_eq!(locales.negotiate(Some("en;q=0.2, fr;q=0.9")).locale(), "fr");
        assert_eq!(locales.negotiate(Some("de")).locale(), "en");
        assert_eq!(locales.negotiate(Some("fr;q=0")).locale(), "en", "q=0 means not acceptable");
        assert_eq!(locales.negotiate(None).locale(), "en");
    }
}
