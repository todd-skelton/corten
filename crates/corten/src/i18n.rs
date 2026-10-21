//! Message catalogs and number, money and date formatting for server-rendered text.
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
//! Formatting follows `en-US` (`Intl.NumberFormat("en-US")` output), which is what the app
//! ships today; other locales fall back to it.

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

/// Digits after the decimal point for an ISO 4217 code.
pub fn currency_exponent(code: &str) -> u32 {
    match code {
        "JPY" | "KRW" | "VND" | "CLP" | "ISK" | "UGX" | "XAF" | "XOF" | "PYG" => 0,
        "BHD" | "KWD" | "JOD" | "OMR" | "TND" | "LYD" | "IQD" => 3,
        _ => 2,
    }
}

/// `1234567` -> `1,234,567`.
pub fn format_number(n: i64) -> String {
    let digits = format_unsigned(n.unsigned_abs());
    if n < 0 {
        format!("-{digits}")
    } else {
        digits
    }
}

fn format_unsigned(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Money from minor units, as `Intl.NumberFormat("en-US", { style: "currency" })` prints it:
/// `format_money(123456, "USD")` is `$1,234.56`; `format_money(-5, "EUR")` is `-€0.05`.
pub fn format_money(minor: i64, currency: &str) -> String {
    let exp = currency_exponent(currency);
    let scale = 10i64.pow(exp);
    let magnitude = minor.unsigned_abs();
    let scale = scale as u64;
    let whole = format_unsigned(magnitude / scale);
    let fraction = if exp == 0 { String::new() } else { format!(".{:0width$}", magnitude % scale, width = exp as usize) };
    let sign = if minor < 0 { "-" } else { "" };
    match currency {
        "USD" => format!("{sign}${whole}{fraction}"),
        "EUR" => format!("{sign}€{whole}{fraction}"),
        "GBP" => format!("{sign}£{whole}{fraction}"),
        "JPY" => format!("{sign}¥{whole}{fraction}"),
        "CAD" => format!("{sign}CA${whole}{fraction}"),
        "AUD" => format!("{sign}A${whole}{fraction}"),
        other => format!("{sign}{other}\u{a0}{whole}{fraction}"),
    }
}

/// Parses a decimal amount (`"12.5"`, as the API sends it) into minor units, exactly.
pub fn parse_minor(amount: &str, currency: &str) -> Option<i64> {
    let exp = currency_exponent(currency) as usize;
    let amount = amount.trim();
    let (negative, amount) = match amount.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, amount),
    };
    let (whole, fraction) = amount.split_once('.').unwrap_or((amount, ""));
    if whole.is_empty() && fraction.is_empty()
        || fraction.len() > exp
        || !whole.bytes().chain(fraction.bytes()).all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let whole: i64 = if whole.is_empty() { 0 } else { whole.parse().ok()? };
    let fraction: i64 = format!("{fraction:0<exp$}").parse().unwrap_or(0);
    let minor = whole.checked_mul(10i64.pow(exp as u32))?.checked_add(fraction)?;
    Some(if negative { -minor } else { minor })
}

/// Basis points as a percent with up to two decimals: `1250` is `12.5%`, `500` is `5%`.
pub fn format_bps_percent(bps: i64) -> String {
    let sign = if bps < 0 { "-" } else { "" };
    let bps = bps.unsigned_abs();
    let fraction = format!("{:02}", bps % 100);
    let fraction = fraction.trim_end_matches('0');
    if fraction.is_empty() {
        format!("{sign}{}%", bps / 100)
    } else {
        format!("{sign}{}.{fraction}%", bps / 100)
    }
}

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// Year, month (1-12) and day for a count of days since 1970-01-01 (proleptic Gregorian).
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

/// A UTC date as `Oct 20, 2026` (`dateStyle: "medium"`), shifted by `offset_minutes` first.
pub fn format_date(unix_seconds: i64, offset_minutes: i32) -> String {
    let local = unix_seconds.saturating_add(i64::from(offset_minutes) * 60);
    let (y, m, d) = civil_from_days(local.div_euclid(86_400));
    format!("{} {d}, {y}", MONTHS[m as usize - 1])
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

    #[test]
    fn money_matches_intl_en_us() {
        assert_eq!(format_money(123_456, "USD"), "$1,234.56");
        assert_eq!(format_money(5, "USD"), "$0.05");
        assert_eq!(format_money(-5, "EUR"), "-€0.05");
        assert_eq!(format_money(1500, "JPY"), "¥1,500");
        assert_eq!(format_money(1000, "CHF"), "CHF\u{a0}10.00");
        assert_eq!(format_money(i64::MIN, "JPY"), "-¥9,223,372,036,854,775,808", "no overflow at the extremes");
        assert_eq!(parse_minor("12.5", "USD"), Some(1250));
        assert_eq!(parse_minor("0.27", "USD"), Some(27));
        assert_eq!(parse_minor("-3", "USD"), Some(-300));
        assert_eq!(parse_minor("1.005", "USD"), None, "more precision than the currency has");
        assert_eq!(parse_minor("1e3", "USD"), None);
        assert_eq!(parse_minor("", "USD"), None);
        assert_eq!(parse_minor("99999999999999999999", "USD"), None);
    }

    #[test]
    fn numbers_percent_and_dates() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(-1_234_567), "-1,234,567");
        assert_eq!(format_number(i64::MIN), "-9,223,372,036,854,775,808");
        assert_eq!(format_bps_percent(1250), "12.5%");
        assert_eq!(format_bps_percent(500), "5%");
        assert_eq!(format_bps_percent(1), "0.01%");
        assert_eq!(format_bps_percent(-25), "-0.25%");
        assert_eq!(format_date(0, 0), "Jan 1, 1970");
        assert_eq!(format_date(1_792_454_400, 0), "Oct 20, 2026");
        assert_eq!(format_date(1_792_454_400, -300), "Oct 19, 2026", "Chicago is behind UTC");
        assert_eq!(format_date(951_782_400, 0), "Feb 29, 2000");
    }
}
