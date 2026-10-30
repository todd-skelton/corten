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
        let shifted = self.hundredfold();
        let mut d = if shifted.scale > digits { shifted.rescale(digits) } else { shifted };
        while d.scale > 0 && d.mantissa % 10 == 0 {
            d.mantissa /= 10;
            d.scale -= 1;
        }
        d
    }

    /// This ratio as a change: the percentage with exactly `digits` fraction digits (0.006 is
    /// 0.60), rounded half away from zero, as Intl writes it given both `minimumFractionDigits`
    /// and `maximumFractionDigits`.
    fn change(self, digits: u8) -> Self {
        let d = self.hundredfold();
        // At most 38 digits in all, as a `Decimal` holds: those of the whole part (before
        // rounding), then the fraction's (`format.js` caps them the same way).
        let whole = d.mantissa / 10u128.pow(u32::from(d.scale));
        let whole_digits = whole.checked_ilog10().map_or(1, |n| n as u8 + 1);
        d.rescale(digits.min(38u8.saturating_sub(whole_digits)))
    }

    /// The value times 100, moving the point.
    fn hundredfold(self) -> Self {
        match self.scale.checked_sub(2) {
            Some(scale) => Decimal { scale, ..self },
            None => Decimal { mantissa: self.mantissa.saturating_mul(10u128.pow(u32::from(2 - self.scale))), scale: 0, ..self },
        }
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
        let scale = usize::from(self.scale);
        // At least one digit before the point. In u64 while the value fits, which every amount
        // a page shows does: a u128 division is a library call, a u64 one a multiplication.
        let mut wide = self.mantissa;
        while wide > u128::from(u64::MAX) {
            end -= 1;
            buf[end] = b'0' + (wide % 10) as u8;
            wide /= 10;
        }
        let mut n = wide as u64;
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

/// What a plural message takes as its count (`t.cards(count)`): an integer, whose plural
/// category in the locale (CLDR's: `one`, `few`, `other`, ...) picks the message's form, and
/// which the message writes as `t.number` does. A [`Decimal`] is not one, since a fraction's
/// category depends on the digits it shows.
#[diagnostic::on_unimplemented(
    message = "a plural message's count is an integer, not {Self}",
    note = "a count is a whole number (u8 to u128, i8 to i128, usize, isize)"
)]
pub trait Count: Number {
    /// The count without its sign, which is what picks the form.
    #[doc(hidden)]
    fn magnitude(&self) -> u64;
}

/// A count's size as a category reads it: itself, or past `u64::MAX` its last six digits past a
/// billion (CLDR's integer rules read no further, and a number that large is past every exact one).
#[inline]
fn magnitude(n: u128) -> u64 {
    u64::try_from(n).unwrap_or(1_000_000_000 + (n % 1_000_000) as u64)
}

macro_rules! count {
    ($($t:ty),*) => {$(
        impl Count for $t {
            #[inline]
            #[allow(clippy::unnecessary_cast)] // i128 as i128
            fn magnitude(&self) -> u64 {
                magnitude((*self as i128).unsigned_abs())
            }
        }
    )*};
}
count!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, usize);

impl Count for u128 {
    #[inline]
    fn magnitude(&self) -> u64 {
        magnitude(*self)
    }
}

impl<T: Count + ?Sized> Count for &T {
    #[inline]
    fn magnitude(&self) -> u64 {
        (**self).magnitude()
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

    /// The code as a number that sorts as the code does, for the tables' binary searches.
    fn key(&self) -> u32 {
        u32::from_be_bytes([0, self.0[0], self.0[1], self.0[2]])
    }

    /// `"USD"`.
    pub fn code(&self) -> &str {
        std::str::from_utf8(&self.0).unwrap_or("XXX")
    }

    /// The fraction digits an amount is shown with: 2, 0 for JPY, 3 for KWD (as Intl shows them).
    pub fn digits(&self) -> u8 {
        let key = self.key();
        DISPLAY_DIGITS.binary_search_by(|(code, _)| code_key(code).cmp(&key)).map_or(2, |i| DISPLAY_DIGITS[i].1)
    }
}

/// A three-letter code in a table as [`Currency::key`] numbers it; the tables hold only such codes
/// (the generator writes them), and anything shorter sorts first.
fn code_key(code: &str) -> u32 {
    match *code.as_bytes() {
        [a, b, c] => u32::from_be_bytes([0, a, b, c]),
        _ => 0,
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

/// A number, an amount of money, a percentage or a change in a locale: what `t.number`,
/// `t.money`, `t.percent` and `t.change` return. It renders in `html!` as text or an attribute value, and is a message
/// argument ([`super::Arg`]).
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
    /// With its sign, as Intl's `signDisplay: "exceptZero"` (`+$1.23`, `-$1.23`, `$0.00`).
    SignedMoney(Currency),
    /// `value` is already Ã— 100 and rounded.
    Percent,
    /// `value` is already times 100 and rounded to its digits; a gain is written with its sign.
    Change,
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
    pub fn signed_money(data: &'static __private::LocaleData, amount: &impl Amount, currency: Currency) -> Self {
        Formatted { data, value: amount.in_digits(currency.digits()), kind: Kind::SignedMoney(currency) }
    }

    #[doc(hidden)]
    pub fn percent(data: &'static __private::LocaleData, ratio: Decimal, digits: u8) -> Self {
        Formatted { data, value: ratio.percent(digits), kind: Kind::Percent }
    }

    #[doc(hidden)]
    pub fn change(data: &'static __private::LocaleData, ratio: Decimal, digits: u8) -> Self {
        Formatted { data, value: ratio.change(digits), kind: Kind::Change }
    }

    fn write<W: Write + ?Sized>(&self, out: &mut W) -> fmt::Result {
        let data = self.data;
        let negative = usize::from(self.value.negative);
        let currency = match self.kind {
            Kind::Number => return self.write_in(data.number[negative], &data.plain, None, out),
            Kind::Percent => return self.write_in(data.percent[negative], &data.percent_separators, None, out),
            // As Intl's `signDisplay: "exceptZero"`: no sign on what rounds to zero, either way.
            Kind::Change => {
                let template = match (self.value.mantissa, self.value.negative) {
                    (0, _) => data.percent[0],
                    (_, true) => data.percent[1],
                    (_, false) => data.percent[2],
                };
                return self.write_in(template, &data.percent_separators, None, out);
            }
            Kind::SignedMoney(currency) => return self.write_signed(currency, out),
            Kind::Money(currency) => currency,
        };
        let key = currency.key();
        match data.currencies.binary_search_by(|(c, ..)| code_key(c).cmp(&key)) {
            // The symbol is in the template: no locale's has a code to place (a test checks).
            Ok(i) => {
                let template = if negative == 1 { data.currencies[i].2 } else { data.currencies[i].1 };
                self.write_in(template, &data.money_separators, None, out)
            }
            // The locale writes this currency by its code.
            Err(_) => self.write_in(data.money[negative], &data.money_separators, Some(currency.code()), out),
        }
    }

    /// An amount with its sign: none on what rounds to zero, either way, and on a gain the loss's
    /// template with the plus sign in place of the minus, as Intl writes it
    /// (`scripts/locale-data.mjs` checks every locale and currency).
    fn write_signed<W: Write + ?Sized>(&self, currency: Currency, out: &mut W) -> fmt::Result {
        let data = self.data;
        let key = currency.key();
        let ([plain, loss], code) = match data.currencies.binary_search_by(|(c, ..)| code_key(c).cmp(&key)) {
            Ok(i) => ([data.currencies[i].1, data.currencies[i].2], None),
            Err(_) => (data.money, Some(currency.code())),
        };
        if self.value.mantissa == 0 {
            return self.write_in(plain, &data.money_separators, code, out);
        }
        if self.value.negative {
            return self.write_in(loss, &data.money_separators, code, out);
        }
        let (minus, plus) = signs(data);
        match loss.split_once(minus) {
            Some((before, after)) => {
                self.write_in(before, &data.money_separators, code, out)?;
                out.write_str(plus)?;
                self.write_in(after, &data.money_separators, code, out)
            }
            None => self.write_in(plain, &data.money_separators, code, out),
        }
    }

    /// `template` with the digits at `\u{1}` and `code` at `\u{2}`, for a template that writes a
    /// currency by its code. Both are single bytes no other character's UTF-8 contains, so the
    /// template is searched as bytes.
    fn write_in<W: Write + ?Sized>(
        &self,
        template: &str,
        separators: &__private::Separators,
        code: Option<&str>,
        out: &mut W,
    ) -> fmt::Result {
        let mut rest = template;
        while let Some(at) = rest.as_bytes().iter().position(|&b| b == 1) {
            write_text(&rest[..at], code, out)?;
            rest = &rest[at + 1..];
            let mut buf = [0u8; 40];
            let (int, frac) = self.value.digits(&mut buf);
            write_grouped(self.data, separators, int, out)?;
            if !frac.is_empty() {
                out.write_str(separators.decimal)?;
                write_digits(self.data, frac, out)?;
            }
        }
        write_text(rest, code, out)
    }
}

/// The locale's minus and plus signs: where its percentage's loss and gain templates differ
/// (`-5%` and `+5%`). `scripts/locale-data.mjs` checks they are Intl's in every locale.
fn signs(data: &__private::LocaleData) -> (&'static str, &'static str) {
    let (loss, gain) = (data.percent[1], data.percent[2]);
    fn same(a: impl Iterator<Item = char>, b: impl Iterator<Item = char>) -> usize {
        a.zip(b).take_while(|(a, b)| a == b).map(|(a, _)| a.len_utf8()).sum()
    }
    let start = same(loss.chars(), gain.chars());
    let end = same(loss[start..].chars().rev(), gain[start..].chars().rev());
    (&loss[start..loss.len() - end], &gain[start..gain.len() - end])
}

/// A template's text, with `code` at each `\u{2}` when there is one.
fn write_text<W: Write + ?Sized>(text: &str, code: Option<&str>, out: &mut W) -> fmt::Result {
    let Some(code) = code else { return out.write_str(text) };
    let mut rest = text;
    while let Some(at) = rest.as_bytes().iter().position(|&b| b == 2) {
        out.write_str(&rest[..at])?;
        out.write_str(code)?;
        rest = &rest[at + 1..];
    }
    out.write_str(rest)
}

/// Money as a person types it in `data`'s locale, read back into the currency's minor units:
/// what `t.parse_money(text, currency)` returns. What `t.money` writes is read back, and what a
/// person types for it: with or without the symbol or the code (on either side), the direction
/// marks, the group separators or the fraction; any space where the locale writes one; the
/// locale's digits or ASCII ones (and Arabic-Indic ones), `.` where the locale's decimal is `٫`.
/// A sign, a group of the wrong size, a grouped number whose first group starts with `0`, another
/// locale's decimal separator, more fraction digits than the currency shows or an amount over
/// `u64` is `None`.
#[doc(hidden)]
pub fn parse_money(data: &__private::LocaleData, text: &str, currency: Currency) -> Option<Minor<u64>> {
    let code = currency.code();
    let template = match data.currencies.binary_search_by(|(c, ..)| (*c).cmp(code)) {
        Ok(i) => data.currencies[i].1,
        Err(_) => data.money[0],
    };
    let (prefix, suffix) = template.split_once('\u{1}')?;
    // Direction marks are dropped; most input has none, and is read where it lies.
    let unmarked: std::borrow::Cow<'_, str> =
        if text.chars().any(is_mark) { text.chars().filter(|c| !is_mark(*c)).collect::<String>().into() } else { text.into() };
    let mut text = unmarked.trim();
    // The symbol before the amount and after it, each where the locale writes one, else the code
    // on either side.
    let mut stripped = false;
    if let Some(rest) = strip_affix(text, prefix, code, true) {
        (text, stripped) = (rest.trim_start(), true);
    }
    if let Some(rest) = strip_affix(text, suffix, code, false) {
        (text, stripped) = (rest.trim_end(), true);
    }
    if !stripped {
        if let Some(rest) = strip_code(text, code, true).or_else(|| strip_code(text, code, false)) {
            text = rest.trim();
        }
    }
    let separators = &data.money_separators;
    let spaced = separators.group.chars().all(is_space);
    // The decimal separator as the locale writes it, or `.` where it writes `٫` (no group is `.`).
    let point = if separators.decimal == "\u{66b}" && separators.group != "." { Some('.') } else { None };
    let (whole, fraction) = match text.split_once(separators.decimal) {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => match point.and_then(|p| text.split_once(p)) {
            Some((whole, fraction)) => (whole, Some(fraction)),
            None => (text, None),
        },
    };
    let mut minor: u64 = 0;
    let mut push = |d: u32| -> Option<()> {
        minor = minor.checked_mul(10)?.checked_add(u64::from(d))?;
        Some(())
    };
    // The integer digits, with the size of each group as typed: the first, the last (`group`) and
    // whether every one between is the secondary size.
    let [primary, secondary, _] = separators.grouping.map(usize::from);
    let (mut groups, mut first, mut group, mut middle, mut digits) = (1_usize, 0, 0, true, 0);
    let mut leading_zero = false;
    let mut rest = whole;
    while let Some(c) = rest.chars().next() {
        if let Some(d) = digit(data, c) {
            push(d)?;
            if digits == 0 {
                leading_zero = d == 0;
            }
            (group, digits) = (group + 1, digits + 1);
            rest = &rest[c.len_utf8()..];
            continue;
        }
        rest = match rest.strip_prefix(separators.group).filter(|_| !separators.group.is_empty()) {
            Some(after) => after,
            None if spaced && is_space(c) => &rest[c.len_utf8()..],
            None => return None,
        };
        if groups == 1 {
            first = group;
        } else {
            middle &= group == secondary;
        }
        (groups, group) = (groups + 1, 0);
    }
    // Grouped as the locale groups: never `0,500` or `00,123`, a decimal comma of another locale.
    if groups > 1 && (first == 0 || first > secondary || group != primary || !middle || leading_zero) {
        return None;
    }
    let shown = usize::from(currency.digits());
    let mut written = 0;
    if let Some(fraction) = fraction {
        for c in fraction.chars() {
            push(digit(data, c)?)?;
            written += 1;
        }
        if written == 0 || written > shown {
            return None;
        }
    }
    if digits == 0 && written == 0 {
        return None;
    }
    for _ in written..shown {
        push(0)?;
    }
    Some(Minor(minor))
}

/// A direction mark (LRM, RLM, ALM): written around some locales' symbols, never typed.
fn is_mark(c: char) -> bool {
    matches!(c, '\u{200e}' | '\u{200f}' | '\u{61c}')
}

/// A space a person types or a locale writes between groups or around a symbol: a tab or a line
/// break is not one there (at either end of the text, any whitespace is trimmed).
fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\u{a0}' | '\u{202f}' | '\u{2009}')
}

/// One character of a symbol as typed: the half-width form of a full-width one (`￥` is `¥`), and
/// ASCII letters in either case (`us$`).
fn fold(c: char) -> char {
    match c {
        '\u{ffe5}' => '\u{a5}',
        '\u{ff04}' => '$',
        c => c.to_ascii_lowercase(),
    }
}

/// `text` without the locale's prefix (`front`) or suffix, a template part where `\u{2}` stands
/// for the code: its direction marks are skipped and a space in it matches any run of spaces,
/// none included. `None` when it is not there or the locale writes nothing there.
fn strip_affix<'a>(text: &'a str, affix: &str, code: &str, front: bool) -> Option<&'a str> {
    let mut want: [char; 24] = ['\0'; 24];
    let mut len = 0;
    for c in affix.chars() {
        let mut put = |c: char| -> Option<()> {
            *want.get_mut(len)? = c;
            len += 1;
            Some(())
        };
        if c == '\u{2}' {
            for c in code.chars() {
                put(c)?;
            }
        } else if !is_mark(c) {
            put(c)?;
        }
    }
    let want = &want[..len];
    if want.iter().all(|c| is_space(*c)) {
        return None;
    }
    let mut chars = text.char_indices();
    let mut next = || if front { chars.next() } else { chars.next_back() };
    let mut pending = next();
    let wanted: &mut dyn Iterator<Item = &char> = if front { &mut want.iter() } else { &mut want.iter().rev() };
    for want in wanted {
        if is_space(*want) {
            while pending.is_some_and(|(_, c)| is_space(c)) {
                pending = next();
            }
        } else if pending.is_some_and(|(_, c)| fold(c) == fold(*want)) {
            pending = next();
        } else {
            return None;
        }
    }
    // `pending` is the first character left on the amount's side, or none.
    Some(match (pending, front) {
        (Some((i, _)), true) => &text[i..],
        (Some((i, c)), false) => &text[..i + c.len_utf8()],
        (None, _) => "",
    })
}

/// `text` without the currency's code at its start (`front`) or end, in either case.
fn strip_code<'a>(text: &'a str, code: &str, front: bool) -> Option<&'a str> {
    let n = code.len();
    if front {
        text.get(..n).filter(|head| head.eq_ignore_ascii_case(code)).map(|_| &text[n..])
    } else {
        let at = text.len().checked_sub(n)?;
        text.get(at..).filter(|tail| tail.eq_ignore_ascii_case(code)).map(|_| &text[..at])
    }
}

/// The value of a digit: ASCII, the locale's own, or Arabic-Indic (either form).
fn digit(data: &__private::LocaleData, c: char) -> Option<u32> {
    [data.zero, '0', '\u{660}', '\u{6f0}'].into_iter().find_map(|zero| (c as u32).checked_sub(zero as u32).filter(|n| *n < 10))
}

/// ASCII digits in the locale's digits.
/// Each is written as a character, which for a `String` is a push: no text to check.
fn write_digits<W: Write + ?Sized>(data: &__private::LocaleData, digits: &[u8], out: &mut W) -> fmt::Result {
    if data.zero == '0' {
        for &d in digits {
            out.write_char(char::from(d))?;
        }
        return Ok(());
    }
    let zero = data.zero as u32;
    for &d in digits {
        out.write_char(char::from_u32(zero + u32::from(d - b'0')).unwrap_or('?'))?;
    }
    Ok(())
}

/// Integer digits with the locale's group separator: a primary group, then secondary ones
/// (12,34,567 in en-IN), and none under the minimum (1234 in es).
fn write_grouped<W: Write + ?Sized>(
    data: &__private::LocaleData,
    s: &__private::Separators,
    int: &[u8],
    out: &mut W,
) -> fmt::Result {
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
        // The locale data has none of `<>&"` (the generator checks); a group separator `'`
        // (de-CH) is safe in text and in the double-quoted attributes `html!` writes.
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
/// is a message argument ([`super::Arg`]).
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

/// How [`Relative`] writes a time: `in 10 hours`, `in 10 hr.` or `in 10h` in en (`Long`,
/// `Short`, `Narrow`), as `Intl.RelativeTimeFormat`'s styles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RelativeStyle {
    Long,
    Short,
    Narrow,
}

impl RelativeStyle {
    /// The style's name in `Intl.RelativeTimeFormat`.
    pub fn name(self) -> &'static str {
        match self {
            RelativeStyle::Long => "long",
            RelativeStyle::Short => "short",
            RelativeStyle::Narrow => "narrow",
        }
    }
}

/// A time relative to another (now) in a locale: what `t.relative(at, now, style)` returns. In the
/// largest unit it has a whole one of, rounded toward zero: seconds under a minute, then minutes,
/// hours, and days from a day (10 h 35 min ahead is `in 10 hours`, 90 s ago is `1 minute ago`).
/// After `now` is the future, at or before it the past. It renders in `html!` as text or an
/// attribute value, and is a message argument ([`super::Arg`]); `corten_ui::RelativeTime` keeps
/// it current in the browser.
#[derive(Clone, Copy)]
pub struct Relative {
    data: &'static __private::LocaleData,
    at: SystemTime,
    now: SystemTime,
    style: RelativeStyle,
}

/// The units a relative time is written in, in seconds (`LocaleData::relative`'s order).
const RELATIVE_UNITS: [u64; 4] = [1, 60, 3600, 86_400];

impl Relative {
    #[doc(hidden)]
    pub fn new(data: &'static __private::LocaleData, at: SystemTime, now: SystemTime, style: RelativeStyle) -> Self {
        Relative { data, at, now, style }
    }

    /// The instant it is relative to now.
    pub fn at(&self) -> SystemTime {
        self.at
    }

    /// Its style.
    pub fn style(&self) -> RelativeStyle {
        self.style
    }

    /// The tag of the locale it is written in (its data's: `en` for `en-PH`).
    pub fn locale(&self) -> &'static str {
        self.data.tag
    }

    /// The instant as `<time datetime>` takes it: `2026-03-22T15:04:05Z` (`..05.250Z` with
    /// milliseconds), in UTC.
    pub fn datetime(&self) -> Datetime {
        Datetime(self.at)
    }

    fn write(&self, out: &mut dyn Write) -> fmt::Result {
        let data = self.data;
        let (future, seconds) = match self.at.duration_since(self.now) {
            Ok(ahead) if !ahead.is_zero() => (true, ahead.as_secs()),
            Ok(_) => (false, 0),
            Err(behind) => (false, behind.duration().as_secs()),
        };
        let unit = RELATIVE_UNITS.iter().rposition(|&size| seconds >= size).unwrap_or(0);
        let n = seconds / RELATIVE_UNITS[unit];
        let category = __private::plural(data, n);
        let template = data.relative[self.style as usize][unit][usize::from(future)][usize::from(category)];
        // Some forms have no number: ar's one second is `قبل ثانية واحدة`.
        let Some((before, after)) = template.split_once('\u{1}') else { return out.write_str(template) };
        out.write_str(before)?;
        let mut buf = [0u8; 24];
        write_grouped(data, &data.plain, itoa(n, false, &mut buf), out)?;
        out.write_str(after)
    }
}

impl Render for Relative {
    fn render(&self, out: &mut String) {
        let _ = self.write(out);
    }
}

impl AttrValue for Relative {
    fn write_attr(&self, name: &str, out: &mut String) {
        out.push_str(name);
        out.push_str("=\"");
        self.render(out);
        out.push('"');
    }
}

impl Display for Relative {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write(f)
    }
}

/// An instant as `<time datetime>` takes it, to the millisecond where it has one:
/// [`Relative::datetime`].
#[derive(Clone, Copy, Debug)]
pub struct Datetime(SystemTime);

impl Display for Datetime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Milliseconds since the epoch, rounded down (before it too).
        let millis = match self.0.duration_since(UNIX_EPOCH) {
            Ok(after) => after.as_millis() as i128,
            Err(before) => {
                let d = before.duration();
                -(d.as_millis() as i128) - i128::from(!d.subsec_nanos().is_multiple_of(1_000_000))
            }
        };
        let seconds = millis.div_euclid(1000) as i64;
        let (year, month, day) = civil_from_days(seconds.div_euclid(86_400));
        let s = seconds.rem_euclid(86_400);
        write!(f, "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)?;
        match millis.rem_euclid(1000) {
            0 => f.write_str("Z"),
            ms => write!(f, ".{ms:03}Z"),
        }
    }
}

impl Render for Datetime {
    fn render(&self, out: &mut String) {
        let _ = write!(out, "{self}");
    }
}

impl AttrValue for Datetime {
    fn write_attr(&self, name: &str, out: &mut String) {
        let _ = write!(out, "{name}=\"{self}\"");
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

/// `t.js().percent(digits)` or `t.js().change(digits)`: the locale's percentage format, for a
/// handler whose JSDoc takes a `PercentFormat` (from `corten`), which writes `format(ratio)` as
/// `t.percent(ratio, digits)` (or `t.change(ratio, digits)`) does.
#[derive(Clone, Copy)]
pub struct PercentFormat {
    data: &'static __private::LocaleData,
    digits: u8,
    /// A change's: exactly `digits` fraction digits, and a sign on a gain.
    change: bool,
}

/// `t.js().money(currency)`: an amount's format, for a handler whose JSDoc takes a `MoneyFormat`
/// (from `corten`), which writes `format(minor)` as `t.money(Minor(minor), currency)` does.
#[derive(Clone, Copy)]
pub struct MoneyFormat {
    data: &'static __private::LocaleData,
    currency: Currency,
    /// With its sign: `t.js().signed_money(currency)`, as `t.signed_money` writes it.
    signed: bool,
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
        PercentFormat { data, digits, change: false }
    }

    #[doc(hidden)]
    pub fn change(data: &'static __private::LocaleData, digits: u8) -> Self {
        PercentFormat { data, digits, change: true }
    }
}

impl MoneyFormat {
    #[doc(hidden)]
    pub fn new(data: &'static __private::LocaleData, currency: Currency) -> Self {
        MoneyFormat { data, currency, signed: false }
    }

    #[doc(hidden)]
    pub fn signed(data: &'static __private::LocaleData, currency: Currency) -> Self {
        MoneyFormat { data, currency, signed: true }
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

    pub(super) fn separators(s: &Separators, templates: &[&str], out: &mut String) {
        out.push_str(",&quot;s&quot;:[");
        json_str(s.decimal, out);
        out.push(',');
        json_str(s.group, out);
        let _ = write!(out, ",{},{},{}]", s.grouping[0], s.grouping[1], s.grouping[2]);
        out.push_str(",&quot;t&quot;:[");
        for (i, template) in templates.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            json_str(template, out);
        }
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

/// A plural message for a script (`t.js().cards()`): the locale's number format, its plural
/// table (the category of each number to 199, then a multiple of a million's) and the message's
/// form for each category (`null` for one the locale does not use), as `format.js` reads them.
pub(crate) fn plural_wire(data: &'static __private::LocaleData, forms: &[&str; 6], out: &mut String) {
    wire::open(data, out);
    wire::separators(&data.plain, &data.number, out);
    out.push_str(",&quot;c&quot;:&quot;");
    // The table as digits, written in one piece (it is 201 of them, twice on a page that sends
    // the message to two handlers).
    let mut table = [0u8; 201];
    for (digit, category) in table.iter_mut().zip(data.plurals.iter().chain([&data.millions])) {
        *digit = b'0' + category;
    }
    out.push_str(std::str::from_utf8(&table).unwrap_or_default());
    out.push_str("&quot;,&quot;m&quot;:[");
    for (i, form) in forms.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(if form.is_empty() { "null" } else { form });
    }
    out.push_str("]}");
}

impl crate::script::Arg<crate::script::ts::NumberFormat> for NumberFormat {
    fn write_arg(&self, _: &crate::script::__private::Ctx, out: &mut String) {
        wire::open(self.data, out);
        wire::separators(&self.data.plain, &self.data.number, out);
        out.push('}');
    }
}

impl crate::script::Arg<crate::script::ts::PercentFormat> for PercentFormat {
    fn write_arg(&self, _: &crate::script::__private::Ctx, out: &mut String) {
        wire::open(self.data, out);
        // A change's has the third template, a gain's, which tells the script it is one.
        let templates = if self.change { &self.data.percent[..] } else { &self.data.percent[..2] };
        wire::separators(&self.data.percent_separators, templates, out);
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
        // A signed amount's has a third template, a gain's, which tells the script it is one.
        if self.signed {
            let (minus, plus) = signs(data);
            let gain = neg.replacen(minus, plus, 1);
            wire::separators(&data.money_separators, &[pos.as_str(), neg.as_str(), gain.as_str()], out);
        } else {
            wire::separators(&data.money_separators, &[pos.as_str(), neg.as_str()], out);
        }
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
    /// The plural category (zero, one, two, few, many, other: 0 to 5) of the whole number `n` in
    /// a locale. `format.js` reads the same table the same way.
    #[inline]
    pub fn plural(data: &LocaleData, n: u64) -> u8 {
        match n {
            0..=199 => data.plurals[n as usize],
            _ if n.is_multiple_of(1_000_000) => data.millions,
            _ => data.plurals[100 + (n % 100) as usize],
        }
    }

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
        /// A percentage, positive and negative, with the locale's sign in place, then a gain as a
        /// change writes it (`+5%` in en, `+%5` in tr).
        pub percent: [&'static str; 3],
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
        /// The plural category (zero, one, two, few, many, other) of each whole number to 199.
        pub plurals: [u8; 200],
        /// A multiple of a million's (fr's "many"); any other number over 199 is as
        /// `100 + n % 100`.
        pub millions: u8,
        /// A time relative to now, with the number at `\u{1}`: by `RelativeStyle`, unit (second,
        /// minute, hour, day), past then future, and plural category.
        pub relative: [[[[&'static str; 6]; 2]; 4]; 3],
    }
}
