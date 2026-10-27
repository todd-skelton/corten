//! Numbers, money and dates in a locale, from the data `messages!` puts into the app for each
//! locale it lists (`scripts/locale-data.mjs` writes it from CLDR, as Intl has it).

use std::fmt::{self, Display, Write};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::display_digits::DISPLAY_DIGITS;
use crate::view::{AttrValue, Render};

/// A decimal number, exact: `21.50` is 2150 with a scale of 2. An amount an API sends as text
/// (`"21.50"`) is read with [`Decimal::parse`], never through a float. The default is 0.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Decimal {
    negative: bool,
    mantissa: u128,
    scale: u8,
}

impl Decimal {
    /// `mantissa` × 10^-`scale`: `Decimal::new(2150, 2)` is 21.50. A scale over 38 is 38.
    pub const fn new(mantissa: i128, scale: u8) -> Self {
        Decimal { negative: mantissa < 0, mantissa: mantissa.unsigned_abs(), scale: if scale > 38 { 38 } else { scale } }
    }

    /// `"21.50"`, `"-3"`, `"0.5"` or `".5"`: an optional sign, digits and an optional fraction, at
    /// most 38 digits in all. Anything else (an exponent, a separator, spaces) is `None`.
    pub fn parse(text: &str) -> Option<Self> {
        let (negative, digits) = match text.as_bytes().first()? {
            b'-' => (true, &text[1..]),
            b'+' => (false, &text[1..]),
            _ => (false, text),
        };
        let (int, frac) = digits.split_once('.').unwrap_or((digits, ""));
        if int.is_empty() && frac.is_empty() || int.len() + frac.len() > 38 {
            return None;
        }
        let mut mantissa: u128 = 0;
        for b in int.bytes().chain(frac.bytes()) {
            if !b.is_ascii_digit() {
                return None;
            }
            mantissa = mantissa * 10 + u128::from(b - b'0');
        }
        Some(Decimal { negative, mantissa, scale: frac.len() as u8 })
    }

    /// This ratio as a percentage, the value Ã— 100, with at most `digits` fraction digits:
    /// rounded half away from zero, without trailing zeros (0.0290 is 2.9), as Intl's
    /// `maximumFractionDigits` writes it.
    fn percent(self, digits: u8) -> Self {
        let shifted = match self.scale.checked_sub(2) {
            Some(scale) => Decimal { scale, ..self },
            None => Decimal { mantissa: self.mantissa.saturating_mul(10u128.pow(u32::from(2 - self.scale))), scale: 0, ..self },
        };
        let mut d = if shifted.scale > digits { shifted.rescale(digits) } else { shifted };
        while d.scale > 0 && d.mantissa % 10 == 0 {
            d.mantissa /= 10;
            d.scale -= 1;
        }
        d
    }

    /// The same value with `digits` fraction digits: padded with zeros, or rounded half away from
    /// zero (21.505 is 21.51), as Intl rounds an amount.
    fn rescale(self, digits: u8) -> Self {
        let mantissa = if digits >= self.scale {
            self.mantissa.saturating_mul(10u128.saturating_pow(u32::from(digits - self.scale)))
        } else {
            let unit = 10u128.pow(u32::from(self.scale - digits));
            let (whole, rest) = (self.mantissa / unit, self.mantissa % unit);
            whole + u128::from(rest >= unit / 2)
        };
        Decimal { negative: self.negative, mantissa, scale: digits }
    }
}

impl std::str::FromStr for Decimal {
    type Err = InvalidDecimal;
    fn from_str(text: &str) -> Result<Self, InvalidDecimal> {
        Decimal::parse(text).ok_or(InvalidDecimal)
    }
}

/// The error of `"x".parse::<Decimal>()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidDecimal;

impl Display for InvalidDecimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("not a decimal number")
    }
}

impl std::error::Error for InvalidDecimal {}

/// As written in code and JSON: `-1234.50`.
impl Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut buf = [0u8; 40];
        let (int, frac) = self.digits(&mut buf);
        if self.negative {
            f.write_char('-')?;
        }
        f.write_str(std::str::from_utf8(int).unwrap_or("0"))?;
        if !frac.is_empty() {
            f.write_char('.')?;
            f.write_str(std::str::from_utf8(frac).unwrap_or(""))?;
        }
        Ok(())
    }
}

impl Decimal {
    /// The ASCII digits before and after the point (at least one before).
    fn digits<'a>(&self, buf: &'a mut [u8; 40]) -> (&'a [u8], &'a [u8]) {
        let mut end = buf.len();
        let mut n = self.mantissa;
        let scale = usize::from(self.scale);
        // At least one digit before the point.
        while n > 0 || buf.len() - end <= scale {
            end -= 1;
            buf[end] = b'0' + (n % 10) as u8;
            n /= 10;
        }
        let digits = &buf[end..];
        digits.split_at(digits.len() - scale)
    }
}

macro_rules! decimal_from {
    ($($t:ty),*) => {$(
        impl From<$t> for Decimal {
            fn from(n: $t) -> Self {
                Decimal { negative: (n as i128) < 0, mantissa: (n as i128).unsigned_abs(), scale: 0 }
            }
        }
        impl Number for $t {
            fn to_decimal(&self) -> Decimal {
                Decimal::from(*self)
            }
        }
        impl Amount for Minor<$t> {
            fn in_digits(&self, digits: u8) -> Decimal {
                Decimal { scale: digits, ..Decimal::from(self.0) }
            }
        }
    )*};
}
decimal_from!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, usize);

impl From<u128> for Decimal {
    fn from(n: u128) -> Self {
        Decimal { negative: false, mantissa: n, scale: 0 }
    }
}

/// What `t.number(n)` takes: an integer, or a [`Decimal`] (written with the fraction digits it
/// has). A float is not one: pass `Decimal::parse` of the text it came as, or a `Decimal::new`.
#[diagnostic::on_unimplemented(
    message = "t.number takes an integer or a corten::i18n::Decimal, not {Self}",
    note = "read a decimal from text with Decimal::parse(\"21.5\"), or make one with Decimal::new(215, 1)"
)]
pub trait Number {
    fn to_decimal(&self) -> Decimal;
}

impl Number for u128 {
    fn to_decimal(&self) -> Decimal {
        Decimal::from(*self)
    }
}

impl Number for Decimal {
    fn to_decimal(&self) -> Decimal {
        *self
    }
}

impl<T: Number + ?Sized> Number for &T {
    fn to_decimal(&self) -> Decimal {
        (**self).to_decimal()
    }
}

/// An amount in a currency's minor unit: `Minor(123450)` in USD is $1,234.50, in JPY ¥123,450.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Minor<T>(pub T);

/// What `t.money(amount, currency)` takes: [`Minor`] units (cents), or a [`Decimal`] in the
/// currency's major unit (`Decimal::parse("21.50")` dollars), rounded half away from zero to the
/// digits the currency is shown with. A bare integer is neither, since it could be either.
#[diagnostic::on_unimplemented(
    message = "t.money takes Minor(cents) or a corten::i18n::Decimal amount, not {Self}",
    note = "an integer could be cents or dollars: write Minor(123450) for $1,234.50, or Decimal::parse(\"1234.50\")"
)]
pub trait Amount {
    /// The amount with `digits` fraction digits.
    #[doc(hidden)]
    fn in_digits(&self, digits: u8) -> Decimal;
}

impl Amount for Minor<u128> {
    fn in_digits(&self, digits: u8) -> Decimal {
        Decimal { negative: false, mantissa: self.0, scale: digits }
    }
}

impl Amount for Decimal {
    fn in_digits(&self, digits: u8) -> Decimal {
        self.rescale(digits)
    }
}

impl<T: Amount + ?Sized> Amount for &T {
    fn in_digits(&self, digits: u8) -> Decimal {
        (**self).in_digits(digits)
    }
}

/// An ISO 4217 currency: `const USD: Currency = Currency::new("USD");`, or
/// `Currency::parse(&order.currency)` for one an API sends.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Currency([u8; 3]);

impl Currency {
    /// A code of three letters, `"USD"`. Anything else panics, which in a `const` fails the build.
    pub const fn new(code: &str) -> Self {
        match Currency::parse(code) {
            Some(currency) => currency,
            None => panic!("a currency is three letters, like \"USD\""),
        }
    }

    /// Three ASCII letters in either case (`"usd"` is USD), or `None`.
    pub const fn parse(code: &str) -> Option<Self> {
        let b = code.as_bytes();
        if b.len() != 3 || !b[0].is_ascii_alphabetic() || !b[1].is_ascii_alphabetic() || !b[2].is_ascii_alphabetic() {
            return None;
        }
        Some(Currency([b[0].to_ascii_uppercase(), b[1].to_ascii_uppercase(), b[2].to_ascii_uppercase()]))
    }

    /// `"USD"`.
    pub fn code(&self) -> &str {
        std::str::from_utf8(&self.0).unwrap_or("XXX")
    }

    /// The fraction digits an amount is shown with: 2, 0 for JPY, 3 for KWD (as Intl shows them).
    pub fn digits(&self) -> u8 {
        DISPLAY_DIGITS.binary_search_by(|(code, _)| code.as_bytes().cmp(&self.0[..])).map_or(2, |i| DISPLAY_DIGITS[i].1)
    }
}

impl Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

/// How much of the date [`t.date`](super) writes, as Intl's `dateStyle` (`en`):
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DateStyle {
    /// `3/22/2026`
    Short,
    /// `Mar 22, 2026`
    Medium,
    /// `March 22, 2026`
    Long,
    /// `Sunday, March 22, 2026`
    Full,
}

/// The month and year without the day, for [`t.date`](super) (`en`): a "Member since" line, a
/// set's release month, a statement's period. Written as Intl writes `{ year, month }`, in the
/// locale's order and with its month names (which some languages inflect differently here).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MonthStyle {
    /// `Mar 2026`
    Short,
    /// `March 2026`
    Long,
}

/// What [`t.date`](super) and `t.js().date` take: a [`DateStyle`], or a [`MonthStyle`] for the
/// month and year alone. `t.date_time` takes a `DateStyle` only, so a month and year with a
/// time does not compile.
pub trait DateOnly: Copy + sealed::Sealed {
    #[doc(hidden)]
    fn index(self) -> u8;
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::DateStyle {}
    impl Sealed for super::MonthStyle {}
}

impl DateOnly for DateStyle {
    #[inline]
    fn index(self) -> u8 {
        self as u8
    }
}

/// After the dates, each date with each time and the times (`LocaleData::dates`).
impl DateOnly for MonthStyle {
    #[inline]
    fn index(self) -> u8 {
        14 + self as u8
    }
}

/// How much of the time [`t.time`](super) writes, with the zone (`en`):
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TimeStyle {
    /// `3:04 PM UTC`
    Short,
    /// `3:04:05 PM UTC`
    Medium,
}

/// The zone a date is written in: its offset from UTC at an instant, in seconds east. The
/// default is [`Utc`], so a page reads the same wherever it is rendered. A zone with daylight
/// saving time (from `chrono-tz` or `jiff`) implements this with its offset at `at`; it is shown as
/// its offset (`GMT-5`), since names like `CDT` are not the same in every locale.
pub trait Zone {
    fn offset(&self, at: SystemTime) -> i32;
}

/// Coordinated Universal Time, written as the locale names it (`UTC`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Utc;

impl Zone for Utc {
    fn offset(&self, _: SystemTime) -> i32 {
        0
    }
}

/// A fixed offset from UTC, in seconds east: `Offset::hours(-5)`, written `GMT-5`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Offset(i32);

impl Offset {
    pub const fn seconds(east: i32) -> Self {
        Offset(east)
    }

    pub const fn hours(east: i8) -> Self {
        Offset(east as i32 * 3600)
    }
}

impl Zone for Offset {
    fn offset(&self, _: SystemTime) -> i32 {
        self.0
    }
}

impl<Z: Zone + ?Sized> Zone for &Z {
    fn offset(&self, at: SystemTime) -> i32 {
        (**self).offset(at)
    }
}

/// An RFC 3339 timestamp, as APIs and databases send them: `2026-03-22T12:00:00Z`,
/// `2026-03-22T12:00:00.123+05:30`, or with a space and a short offset (`2026-02-23
/// 21:26:00.123456+00`, Postgres' text). `None` for anything else, or a date that does not exist.
pub fn parse_timestamp(text: &str) -> Option<SystemTime> {
    let b = text.as_bytes();
    let num = |range: std::ops::Range<usize>| -> Option<i64> {
        let digits = b.get(range)?;
        digits.iter().try_fold(0i64, |n, d| d.is_ascii_digit().then(|| n * 10 + i64::from(d - b'0')))
    };
    let (year, month, day) = (num(0..4)?, num(5..7)?, num(8..10)?);
    if b.get(4) != Some(&b'-') || b.get(7) != Some(&b'-') || !matches!(b.get(10), Some(b'T' | b't' | b' ')) {
        return None;
    }
    let (hour, minute, second) = (num(11..13)?, num(14..16)?, num(17..19)?);
    if b.get(13) != Some(&b':') || b.get(16) != Some(&b':') {
        return None;
    }
    let mut i = 19;
    let mut nanos = 0u32;
    if b.get(i) == Some(&b'.') {
        i += 1;
        let start = i;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            if i - start < 9 {
                nanos = nanos * 10 + u32::from(b[i] - b'0');
            }
            i += 1;
        }
        if i == start {
            return None;
        }
        nanos *= 10u32.pow(9u32.saturating_sub((i - start) as u32));
    }
    let offset = match b.get(i)? {
        b'Z' | b'z' if i + 1 == b.len() => 0,
        &sign @ (b'+' | b'-') => {
            let hours = num(i + 1..i + 3)?;
            let minutes = match &b[i + 3..] {
                [] => 0,
                [b':', ..] if b.len() == i + 6 => num(i + 4..i + 6)?,
                _ if b.len() == i + 5 => num(i + 3..i + 5)?,
                _ => return None,
            };
            if hours > 23 || minutes > 59 {
                return None;
            }
            let east = hours * 3600 + minutes * 60;
            if sign == b'-' {
                -east
            } else {
                east
            }
        }
        _ => return None,
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_in_month = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    if !(1..=12).contains(&month) || day < 1 || day > days_in_month[month as usize - 1] || hour > 23 || minute > 59 || second > 60
    {
        return None;
    }
    let seconds = days_from_civil(year, month as u32, day as u32) * 86_400 + hour * 3600 + minute * 60 + second.min(59) - offset;
    let at = if seconds >= 0 {
        UNIX_EPOCH.checked_add(Duration::new(seconds as u64, nanos))?
    } else {
        UNIX_EPOCH
            .checked_sub(Duration::from_secs(seconds.unsigned_abs()))?
            .checked_add(Duration::from_nanos(u64::from(nanos)))?
    };
    Some(at)
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (i64::from(month) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The date of a day since 1970-01-01: (year, month, day).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

#[cfg(test)]
pub(super) fn unix_seconds_for_test(at: SystemTime) -> i64 {
    unix_seconds(at)
}

/// Seconds since 1970-01-01, negative before it.
fn unix_seconds(at: SystemTime) -> i64 {
    match at.duration_since(UNIX_EPOCH) {
        Ok(after) => after.as_secs() as i64,
        Err(before) => {
            let d = before.duration();
            -(d.as_secs() as i64) - i64::from(d.subsec_nanos() > 0)
        }
    }
}

/// A number, an amount of money or a percentage in a locale: what `t.number`, `t.money` and
/// `t.percent` return. It renders in `html!` as text or an attribute value, and is a message
/// argument like any `Display` value.
#[derive(Clone, Copy)]
pub struct Formatted {
    data: &'static __private::LocaleData,
    value: Decimal,
    kind: Kind,
}

#[derive(Clone, Copy)]
enum Kind {
    Number,
    Money(Currency),
    /// `value` is already Ã— 100 and rounded.
    Percent,
}

impl Formatted {
    #[doc(hidden)]
    pub fn number(data: &'static __private::LocaleData, value: Decimal) -> Self {
        Formatted { data, value, kind: Kind::Number }
    }

    #[doc(hidden)]
    pub fn money(data: &'static __private::LocaleData, amount: &impl Amount, currency: Currency) -> Self {
        Formatted { data, value: amount.in_digits(currency.digits()), kind: Kind::Money(currency) }
    }

    #[doc(hidden)]
    pub fn percent(data: &'static __private::LocaleData, ratio: Decimal, digits: u8) -> Self {
        Formatted { data, value: ratio.percent(digits), kind: Kind::Percent }
    }

    fn write(&self, out: &mut dyn Write) -> fmt::Result {
        let data = self.data;
        let negative = usize::from(self.value.negative);
        let currency = match self.kind {
            Kind::Number => return self.write_in(data.number[negative], &data.plain, "", out),
            Kind::Percent => return self.write_in(data.percent[negative], &data.percent_separators, "", out),
            Kind::Money(currency) => currency,
        };
        let code = currency.code();
        let template = match data.currencies.binary_search_by(|(c, ..)| (*c).cmp(code)) {
            Ok(i) if negative == 1 => data.currencies[i].2,
            Ok(i) => data.currencies[i].1,
            // The locale writes this currency by its code.
            Err(_) => data.money[negative],
        };
        self.write_in(template, &data.money_separators, code, out)
    }

    /// `template` with the digits at `\u{1}` and `code` at `\u{2}`.
    fn write_in(&self, template: &str, separators: &__private::Separators, code: &str, out: &mut dyn Write) -> fmt::Result {
        let mut start = 0;
        for (i, c) in template.char_indices() {
            if c != '\u{1}' && c != '\u{2}' {
                continue;
            }
            out.write_str(&template[start..i])?;
            start = i + 1;
            if c == '\u{2}' {
                out.write_str(code)?;
                continue;
            }
            let mut buf = [0u8; 40];
            let (int, frac) = self.value.digits(&mut buf);
            write_grouped(self.data, separators, int, out)?;
            if !frac.is_empty() {
                out.write_str(separators.decimal)?;
                write_digits(self.data, frac, out)?;
            }
        }
        out.write_str(&template[start..])
    }
}

/// ASCII digits in the locale's digits.
fn write_digits(data: &__private::LocaleData, digits: &[u8], out: &mut dyn Write) -> fmt::Result {
    if data.zero == '0' {
        return out.write_str(std::str::from_utf8(digits).unwrap_or(""));
    }
    let zero = data.zero as u32;
    for d in digits {
        out.write_char(char::from_u32(zero + u32::from(d - b'0')).unwrap_or('?'))?;
    }
    Ok(())
}

/// Integer digits with the locale's group separator: a primary group, then secondary ones
/// (12,34,567 in en-IN), and none under the minimum (1234 in es).
fn write_grouped(data: &__private::LocaleData, s: &__private::Separators, int: &[u8], out: &mut dyn Write) -> fmt::Result {
    let [primary, secondary, min] = s.grouping.map(usize::from);
    if int.len() < primary + min {
        return write_digits(data, int, out);
    }
    let head_len = int.len() - primary;
    let (head, last) = int.split_at(head_len);
    // The secondary groups of `head`, from the left: the first may be shorter.
    let first = match head_len % secondary {
        0 => secondary,
        n => n,
    };
    write_digits(data, &head[..first], out)?;
    let mut i = first;
    while i < head_len {
        out.write_str(s.group)?;
        write_digits(data, &head[i..i + secondary], out)?;
        i += secondary;
    }
    out.write_str(s.group)?;
    write_digits(data, last, out)
}
impl Render for Formatted {
    fn render(&self, out: &mut String) {
        // The locale data has no character HTML would read (the generator checks).
        let _ = self.write(out);
    }
}

impl AttrValue for Formatted {
    fn write_attr(&self, name: &str, out: &mut String) {
        out.push_str(name);
        out.push_str("=\"");
        self.render(out);
        out.push('"');
    }
}

impl Display for Formatted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write(f)
    }
}

/// A date, a time or both in a locale: what `t.date`, `t.time` and `t.date_time` return. In UTC
/// unless [`When::zone`] says otherwise. It renders in `html!` as text or an attribute value, and
/// is a message argument like any `Display` value.
#[derive(Clone, Copy)]
pub struct When {
    data: &'static __private::LocaleData,
    at: SystemTime,
    offset: i32,
    style: u8,
}

impl When {
    #[doc(hidden)]
    pub fn date(data: &'static __private::LocaleData, at: SystemTime, date: impl DateOnly) -> Self {
        When { data, at, offset: 0, style: date.index() }
    }

    #[doc(hidden)]
    pub fn date_time(data: &'static __private::LocaleData, at: SystemTime, date: DateStyle, time: TimeStyle) -> Self {
        When { data, at, offset: 0, style: 4 + date as u8 * 2 + time as u8 }
    }

    #[doc(hidden)]
    pub fn time(data: &'static __private::LocaleData, at: SystemTime, time: TimeStyle) -> Self {
        When { data, at, offset: 0, style: 12 + time as u8 }
    }
    /// In `zone` instead of UTC: `t.date_time(at, ..).zone(&viewer.zone)`.
    pub fn zone(mut self, zone: impl Zone) -> Self {
        self.offset = zone.offset(self.at);
        self
    }

    fn write(&self, out: &mut dyn Write) -> fmt::Result {
        use __private::Field as F;
        let data = self.data;
        let local = unix_seconds(self.at) + i64::from(self.offset);
        let days = local.div_euclid(86_400);
        let second_of_day = local.rem_euclid(86_400);
        let (year, month, day) = civil_from_days(days);
        let weekday = (days + 4).rem_euclid(7) as usize;
        let hour = (second_of_day / 3600) as u32;
        let minute = (second_of_day / 60 % 60) as u32;
        let second = (second_of_day % 60) as u32;
        let mut buf = [0u8; 24];
        let mut number = |n: i64, two: bool, out: &mut dyn Write| -> fmt::Result {
            let text = itoa(n.unsigned_abs(), two, &mut buf);
            if n < 0 {
                out.write_char('-')?;
            }
            write_digits(data, text, out)
        };
        for field in data.dates[usize::from(self.style)] {
            match *field {
                F::Text(text) => out.write_str(text)?,
                F::Year => number(year, false, out)?,
                F::Year2 => number(year.rem_euclid(100), true, out)?,
                F::Month => number(i64::from(month), false, out)?,
                F::Month2 => number(i64::from(month), true, out)?,
                F::MonthName(at) => out.write_str(data.names[usize::from(at) + month as usize - 1])?,
                F::Day => number(i64::from(day), false, out)?,
                F::Day2 => number(i64::from(day), true, out)?,
                F::Weekday(at) => out.write_str(data.names[usize::from(at) + weekday])?,
                F::Hour(cycle, two) => {
                    let h = match cycle {
                        12 if hour.is_multiple_of(12) => 12,
                        12 | 11 => hour % 12,
                        24 if hour == 0 => 24,
                        _ => hour,
                    };
                    number(i64::from(h), two, out)?
                }
                F::Minute => number(i64::from(minute), false, out)?,
                F::Minute2 => number(i64::from(minute), true, out)?,
                F::Second => number(i64::from(second), false, out)?,
                F::Second2 => number(i64::from(second), true, out)?,
                F::Period(at) => out.write_str(data.names[usize::from(at) + hour as usize])?,
                F::Zone => {
                    if self.offset == 0 {
                        out.write_str(data.utc)?;
                        continue;
                    }
                    let east = self.offset.unsigned_abs();
                    let minutes = east % 3600 / 60;
                    let template = data.gmt[usize::from(minutes != 0) * 2 + usize::from(self.offset < 0)];
                    let mut start = 0;
                    for (i, c) in template.char_indices() {
                        if c == '\u{1}' || c == '\u{3}' {
                            out.write_str(&template[start..i])?;
                            start = i + 1;
                            let (n, two) = if c == '\u{1}' { (east / 3600, false) } else { (minutes, true) };
                            number(i64::from(n), two, out)?;
                        }
                    }
                    out.write_str(&template[start..])?;
                }
            }
        }
        Ok(())
    }
}

/// `n` in ASCII, two digits at least when `two`.
fn itoa(mut n: u64, two: bool, buf: &mut [u8; 24]) -> &[u8] {
    let mut start = buf.len();
    loop {
        start -= 1;
        buf[start] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 && (!two || buf.len() - start >= 2) {
            break;
        }
    }
    &buf[start..]
}

impl Render for When {
    fn render(&self, out: &mut String) {
        let _ = self.write(out);
    }
}

impl AttrValue for When {
    fn write_attr(&self, name: &str, out: &mut String) {
        out.push_str(name);
        out.push_str("=\"");
        self.render(out);
        out.push('"');
    }
}

impl Display for When {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write(f)
    }
}

/// `t.js().number()`: the locale's number format, for a handler whose JSDoc takes a
/// `NumberFormat` (from `corten`), which writes `format(n)` as `t.number(n)` does.
#[derive(Clone, Copy)]
pub struct NumberFormat {
    data: &'static __private::LocaleData,
}

/// `t.js().percent(digits)`: the locale's percentage format, for a handler whose JSDoc takes a
/// `PercentFormat` (from `corten`), which writes `format(ratio)` as `t.percent(ratio, digits)`
/// does.
#[derive(Clone, Copy)]
pub struct PercentFormat {
    data: &'static __private::LocaleData,
    digits: u8,
}

/// `t.js().money(currency)`: an amount's format, for a handler whose JSDoc takes a `MoneyFormat`
/// (from `corten`), which writes `format(minor)` as `t.money(Minor(minor), currency)` does.
#[derive(Clone, Copy)]
pub struct MoneyFormat {
    data: &'static __private::LocaleData,
    currency: Currency,
}

/// `t.js().date(style)` (or `time`, `date_time`): a date's format, for a handler whose JSDoc
/// takes a `DateFormat` (from `corten`), which writes `format(at)` (a `Date`, milliseconds or ISO
/// text) as `t.date(at, style)` does. In UTC unless [`DateFormat::zone`] gives an offset; a zone
/// with daylight saving time is not one, since its offset depends on the instant.
#[derive(Clone, Copy)]
pub struct DateFormat {
    data: &'static __private::LocaleData,
    style: u8,
    offset: i32,
}

impl NumberFormat {
    #[doc(hidden)]
    pub fn new(data: &'static __private::LocaleData) -> Self {
        NumberFormat { data }
    }
}

impl PercentFormat {
    #[doc(hidden)]
    pub fn new(data: &'static __private::LocaleData, digits: u8) -> Self {
        PercentFormat { data, digits }
    }
}

impl MoneyFormat {
    #[doc(hidden)]
    pub fn new(data: &'static __private::LocaleData, currency: Currency) -> Self {
        MoneyFormat { data, currency }
    }
}

impl DateFormat {
    #[doc(hidden)]
    pub fn date(data: &'static __private::LocaleData, date: impl DateOnly) -> Self {
        DateFormat { data, style: date.index(), offset: 0 }
    }

    #[doc(hidden)]
    pub fn date_time(data: &'static __private::LocaleData, date: DateStyle, time: TimeStyle) -> Self {
        DateFormat { data, style: 4 + date as u8 * 2 + time as u8, offset: 0 }
    }

    #[doc(hidden)]
    pub fn time(data: &'static __private::LocaleData, time: TimeStyle) -> Self {
        DateFormat { data, style: 12 + time as u8, offset: 0 }
    }

    /// At a fixed offset instead of UTC.
    pub fn zone(mut self, offset: Offset) -> Self {
        self.offset = offset.0;
        self
    }
}

/// The `%{..}` argument of `data-on` (`crates/corten/js/format.js` reads it), escaped for the
/// attribute.
mod wire {
    use super::__private::{Field, LocaleData, Separators};
    use crate::script::__private::json_str;
    use std::fmt::Write;

    pub(super) fn open(data: &LocaleData, out: &mut String) {
        // The runtime's formatter is a module of its own, which this page now loads.
        crate::script::uses(crate::script::FORMAT_MODULE);
        out.push_str("%{&quot;z&quot;:");
        let mut zero = [0u8; 4];
        json_str(data.zero.encode_utf8(&mut zero), out);
    }

    pub(super) fn separators(s: &Separators, templates: [&str; 2], out: &mut String) {
        out.push_str(",&quot;s&quot;:[");
        json_str(s.decimal, out);
        out.push(',');
        json_str(s.group, out);
        let _ = write!(out, ",{},{},{}]", s.grouping[0], s.grouping[1], s.grouping[2]);
        out.push_str(",&quot;t&quot;:[");
        json_str(templates[0], out);
        out.push(',');
        json_str(templates[1], out);
        out.push(']');
    }

    pub(super) fn fields(data: &LocaleData, fields: &[Field], out: &mut String) {
        out.push_str(",&quot;f&quot;:[");
        for (i, field) in fields.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let (code, names) = match *field {
                Field::Text(text) => {
                    out.push_str("[&quot;&quot;,");
                    json_str(text, out);
                    out.push(']');
                    continue;
                }
                Field::Hour(cycle, two) => {
                    let _ = write!(out, "[&quot;h&quot;,{cycle},{}]", u8::from(two));
                    continue;
                }
                // A name for each of the 24 hours, as runs: ["P","AM",12,"PM",24].
                Field::Period(at) => {
                    out.push_str("[&quot;P&quot;");
                    let names = &data.names[usize::from(at)..usize::from(at) + 24];
                    for (hour, name) in names.iter().enumerate() {
                        if names.get(hour + 1) != Some(name) {
                            out.push(',');
                            json_str(name, out);
                            let _ = write!(out, ",{}", hour + 1);
                        }
                    }
                    out.push(']');
                    continue;
                }
                Field::Year => ("y", None),
                Field::Year2 => ("yy", None),
                Field::Month => ("M", None),
                Field::Month2 => ("MM", None),
                Field::MonthName(at) => ("N", Some((at, 12))),
                Field::Day => ("d", None),
                Field::Day2 => ("dd", None),
                Field::Weekday(at) => ("W", Some((at, 7))),
                Field::Minute => ("m", None),
                Field::Minute2 => ("mm", None),
                Field::Second => ("s", None),
                Field::Second2 => ("ss", None),
                Field::Zone => ("z", None),
            };
            let _ = write!(out, "[&quot;{code}&quot;");
            if let Some((at, count)) = names {
                for name in &data.names[usize::from(at)..usize::from(at) + count] {
                    out.push(',');
                    json_str(name, out);
                }
            }
            out.push(']');
        }
        out.push(']');
    }
}

impl crate::script::Arg<crate::script::ts::NumberFormat> for NumberFormat {
    fn write_arg(&self, _: &crate::script::__private::Ctx, out: &mut String) {
        wire::open(self.data, out);
        wire::separators(&self.data.plain, self.data.number, out);
        out.push('}');
    }
}

impl crate::script::Arg<crate::script::ts::PercentFormat> for PercentFormat {
    fn write_arg(&self, _: &crate::script::__private::Ctx, out: &mut String) {
        wire::open(self.data, out);
        wire::separators(&self.data.percent_separators, self.data.percent, out);
        let _ = write!(out, ",&quot;p&quot;:{}}}", self.digits);
    }
}

impl crate::script::Arg<crate::script::ts::MoneyFormat> for MoneyFormat {
    fn write_arg(&self, _: &crate::script::__private::Ctx, out: &mut String) {
        let data = self.data;
        let code = self.currency.code();
        // The currency in place: its own templates, or the locale's for one written by its code.
        let [pos, neg] = match data.currencies.binary_search_by(|(c, ..)| (*c).cmp(code)) {
            Ok(i) => [data.currencies[i].1.to_string(), data.currencies[i].2.to_string()],
            Err(_) => data.money.map(|t| t.replace('\u{2}', code)),
        };
        wire::open(data, out);
        wire::separators(&data.money_separators, [&pos, &neg], out);
        let _ = write!(out, ",&quot;d&quot;:{}}}", self.currency.digits());
    }
}

impl crate::script::Arg<crate::script::ts::DateFormat> for DateFormat {
    fn write_arg(&self, _: &crate::script::__private::Ctx, out: &mut String) {
        let data = self.data;
        wire::open(data, out);
        let fields = data.dates[usize::from(self.style)];
        wire::fields(data, fields, out);
        let _ = write!(out, ",&quot;o&quot;:{}", self.offset);
        // The zone's names, for a pattern that shows it.
        if fields.iter().any(|f| matches!(f, __private::Field::Zone)) {
            out.push_str(",&quot;u&quot;:");
            crate::script::__private::json_str(data.utc, out);
            out.push_str(",&quot;g&quot;:[");
            for (i, template) in data.gmt.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                crate::script::__private::json_str(template, out);
            }
            out.push(']');
        }
        out.push('}');
    }
}

/// The data `messages!` writes for each locale. Not a public API.
#[doc(hidden)]
pub mod __private {
    /// How a locale writes the digits of a number.
    pub struct Separators {
        pub decimal: &'static str,
        pub group: &'static str,
        /// The primary group's size, the others', and the fewest digits before the first
        /// separator (1234 is not grouped when it is 2).
        pub grouping: [u8; 3],
    }

    /// A field of a date pattern. Names are indexes into `LocaleData::names`: 12 months, 7
    /// weekdays from Sunday, or a period for each of the 24 hours.
    pub enum Field {
        Text(&'static str),
        Year,
        Year2,
        Month,
        Month2,
        MonthName(u16),
        Day,
        Day2,
        Weekday(u16),
        /// The cycle (12: 1-12, 11: 0-11, 23: 0-23, 24: 1-24), and two digits.
        Hour(u8, bool),
        Minute,
        Minute2,
        Second,
        Second2,
        Period(u16),
        Zone,
    }

    /// One locale's data. Templates have the digits at `\u{1}` and a currency's code at `\u{2}`.
    pub struct LocaleData {
        pub tag: &'static str,
        /// The locale's zero; its digits follow it.
        pub zero: char,
        pub plain: Separators,
        pub money_separators: Separators,
        /// A number, positive and negative.
        pub number: [&'static str; 2],
        /// A percentage's separators (bn groups a number 12,34,567 and a percentage 1,234,567%).
        pub percent_separators: Separators,
        /// A percentage, positive and negative, with the locale's sign in place.
        pub percent: [&'static str; 2],
        /// An amount in a currency written by its code (`CHF 5.00`), positive and negative.
        pub money: [&'static str; 2],
        /// Each currency written otherwise, by code: positive and negative, the symbol in place.
        pub currencies: &'static [(&'static str, &'static str, &'static str)],
        pub names: &'static [&'static str],
        /// Four dates, each with each time, two times (`DateStyle`, `TimeStyle` order), then the
        /// month and year (`MonthStyle`).
        pub dates: [&'static [Field]; 16],
        pub utc: &'static str,
        /// An offset (hours at `\u{1}`, minutes at `\u{3}`): whole hours east, west, then with
        /// minutes, east and west.
        pub gmt: [&'static str; 4],
    }
}
