// Writes the number, currency, date and relative time data `messages!` puts into an app for each locale it
// lists, from Node's Intl (ICU's CLDR), and test cases Intl formatted for the Rust side to match:
//   crates/corten-macros/src/locale_data.rs  one entry per locale (an alias for a locale whose
//                                            data is another's), as the tokens of a LocaleData
//   crates/corten/src/i18n/display_digits.rs the fraction digits Intl shows per currency
//   crates/corten/src/i18n/cases.tsv         locale, case, input, what Intl writes
// Before writing, it formats many values with the extracted data (its own copy of the Rust
// algorithm) and stops at the first one Intl writes differently.
// Run: node scripts/locale-data.mjs (the output depends on Node's ICU; the files say which).
import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

// Languages with CLDR's modern coverage, and the regions whose forms differ from the language's.
const LANGUAGES = `af am ar as az be bg bn bs ca cs cy da de el en es et eu fa fi fil fr ga gl gu he hi hr hu hy id is it ja ka kk km kn ko ky lo lt lv mk ml mn mr ms my nb ne nl or pa pl ps pt ro ru si sk sl sq sr sv sw ta te th tr uk ur uz vi zh zu`.split(" ");
const REGIONS = `en-GB en-AU en-CA en-IN en-IE en-NZ en-ZA en-SG en-PH es-MX es-419 es-AR es-CO es-CL es-US fr-CA fr-CH fr-BE de-AT de-CH it-CH nl-BE pt-PT zh-Hant zh-TW zh-HK sr-Latn ar-EG ar-SA ar-AE ar-MA ms-SG sv-FI`.split(" ");

// The styles, in the order of LocaleData::dates: four dates, each date with each time, two times,
// then the month and year without the day, short and long, then the month and day without the
// year, short and long (each after the others, so their indices stay as they were).
const DATE = [
  { year: "numeric", month: "numeric", day: "numeric" },
  { year: "numeric", month: "short", day: "numeric" },
  { year: "numeric", month: "long", day: "numeric" },
  { weekday: "long", year: "numeric", month: "long", day: "numeric" },
];
const TIME = [
  { hour: "numeric", minute: "2-digit", timeZoneName: "short" },
  { hour: "numeric", minute: "2-digit", second: "2-digit", timeZoneName: "short" },
];
const MONTH = [
  { year: "numeric", month: "short" },
  { year: "numeric", month: "long" },
];
const DAY = [
  { month: "short", day: "numeric" },
  { month: "long", day: "numeric" },
];
// Plural categories, in the order of LocaleData::relative's templates.
const PLURALS = ["zero", "one", "two", "few", "many", "other"];
// A time relative to now: the styles, and the units with their seconds (`RelativeStyle` order).
const RELATIVE_STYLES = ["long", "short", "narrow"];
const RELATIVE_UNITS = ["second", "minute", "hour", "day"];
const RELATIVE_SECONDS = [1, 60, 3600, 86400];
const BAGS = [...DATE, ...DATE.flatMap((d) => TIME.map((t) => ({ ...d, ...t }))), ...TIME, ...MONTH, ...DAY];

const at = (y, mo, d, h = 0, mi = 0, s = 0) => new Date(Date.UTC(y, mo - 1, d, h, mi, s));
const SAMPLE = at(2026, 3, 22, 15, 4, 5); // a Sunday
const strip = (s) => s.replace(/[\u200e\u200f\u061c]/g, "");

function dtf(tag, bag, timeZone = "UTC") {
  return new Intl.DateTimeFormat(tag, { ...bag, timeZone, calendar: "gregory" });
}
const part = (parts, type) => parts.find((p) => p.type === type)?.value;

function localeData(tag) {
  const nf = new Intl.NumberFormat(tag);
  const nu = nf.resolvedOptions().numberingSystem;
  if (dtf(tag, BAGS[0]).resolvedOptions().numberingSystem !== nu) throw new Error(`${tag}: dates and numbers use different digits`);
  const zero = strip(nf.format(0));
  for (let i = 0; i < 10; i++) {
    if (strip(nf.format(i)) !== String.fromCodePoint(zero.codePointAt(0) + i)) throw new Error(`${tag}: digits are not contiguous`);
  }
  const digits = (ascii) => [...ascii].map((c) => (c >= "0" && c <= "9" ? String.fromCodePoint(zero.codePointAt(0) + +c) : c)).join("");

  // Numbers and amounts of money: separators and grouping (a locale may write them differently:
  // fr-CH has 1 234,5 and 1'234.50 CHF, ml groups amounts by thousands only).
  // `per` is what the format multiplies by (100 for a percentage).
  const separators = (f, per = 1) => {
    const np = f.formatToParts(-1234567.5 / per);
    const groups = f.formatToParts(1234567890 / per).filter((p) => p.type === "integer").map((p) => p.value.length);
    const primary = groups[groups.length - 1];
    const secondary = groups.length > 2 ? groups[groups.length - 2] : primary;
    const grouped = (v) => f.formatToParts(v / per).some((p) => p.type === "group");
    const min = grouped(1234) ? 1 : grouped(12345) ? 2 : 3;
    return { decimal: part(np, "decimal"), group: part(np, "group"), grouping: [primary, secondary, min] };
  };
  const plain = separators(nf);
  const ofMoney = separators(new Intl.NumberFormat(tag, { style: "currency", currency: "XYZ" }));
  const template = (parts) => {
    let out = "";
    let inNumber = false;
    for (const p of parts) {
      if (["integer", "group", "decimal", "fraction"].includes(p.type)) {
        if (!inNumber) out += "\u0001";
        inNumber = true;
        continue;
      }
      inNumber = false;
      out += p.type === "currency" ? "\u0002" : p.value;
    }
    return out;
  };
  const number = [template(nf.formatToParts(5)), template(nf.formatToParts(-5))];
  // A percentage, with the locale's sign in place (`5 %` in fr, `%5` in tr) and its own grouping
  // (bn groups a number 12,34,567 and a percentage 1,234,567%).
  const pf = new Intl.NumberFormat(tag, { style: "percent" });
  const ofPercent = separators(new Intl.NumberFormat(tag, { style: "percent", maximumFractionDigits: 2 }), 100);
  // A change (signDisplay "exceptZero") writes a gain with the locale's plus sign: `+5%`, `+5 %`.
  const change = new Intl.NumberFormat(tag, { style: "percent", signDisplay: "exceptZero" });
  const percent = [template(pf.formatToParts(5)), template(pf.formatToParts(-5)), template(change.formatToParts(5))];

  // Money: the form of a currency whose symbol is its code (`CHF 5.00`, and any code Intl does not
  // know), positive and negative, with the code at \u0002; and each currency written otherwise
  // (`$5.00`, `5,00 $US`), whole, with its symbol in place.
  const generic = (cur, v) => template(new Intl.NumberFormat(tag, { style: "currency", currency: cur }).formatToParts(v));
  const money = [generic("XYZ", 5), generic("XYZ", -5)];
  const symbols = [];
  for (const code of Intl.supportedValuesOf("currency")) {
    const forms = [5, -5].map((v) => generic(code, v).replace("\u0002", part(new Intl.NumberFormat(tag, { style: "currency", currency: code }).formatToParts(v), "currency")));
    if (forms.some((f, i) => f !== money[i].replace("\u0002", code))) symbols.push([code, ...forms]);
  }
  // Dates: each style's fields; names are 12 months, 7 weekdays (from Sunday) or 24 periods.
  const names = [];
  const nameAt = (list) => {
    for (let i = 0; i + list.length <= names.length; i++) {
      if (list.every((n, j) => names[i + j] === n)) return i;
    }
    names.push(...list);
    return names.length - list.length;
  };
  const dates = BAGS.map((bag) => {
    const f = dtf(tag, bag);
    const fields = [];
    // V8's format() writes a narrow no-break space in a date as a space (formatToParts does
    // not); pages show format()'s.
    const partsOf = (date) => f.formatToParts(date).map((x) => ({ type: x.type, value: x.value.replaceAll("\u202f", " ") }));
    for (const p of partsOf(SAMPLE)) {
      const v = p.value;
      const of = (date) => part(partsOf(date), p.type);
      switch (p.type) {
        case "literal":
          fields.push(["Text", v]);
          break;
        case "year":
          if (v === digits("2026")) fields.push(["Year"]);
          else if (v === digits("26")) fields.push(["Year2"]);
          else throw new Error(`${tag}: year ${v}`);
          break;
        case "month":
          if (v === digits("3")) fields.push([of(at(2026, 5, 22)) === digits("05") ? "Month2" : "Month"]);
          else if (v === digits("03")) fields.push(["Month2"]);
          else fields.push(["MonthName", nameAt(Array.from({ length: 12 }, (_, m) => of(at(2026, m + 1, 22, 15, 4, 5))))]);
          break;
        case "day":
          if (v !== digits("22")) throw new Error(`${tag}: day ${v}`);
          fields.push([of(at(2026, 3, 5)) === digits("05") ? "Day2" : "Day"]);
          break;
        case "weekday":
          fields.push(["Weekday", nameAt(Array.from({ length: 7 }, (_, d) => of(at(2026, 3, 22 + d, 15, 4, 5))))]);
          break;
        case "hour": {
          const h = (hour) => of(at(2026, 3, 22, hour, 4, 5));
          const cycle = v === digits("15") ? (h(0) === digits("24") ? 24 : 23) : h(0) === digits("0") || h(0) === digits("00") ? 11 : 12;
          const two = h(5) === digits("05");
          fields.push(["Hour", cycle, two]);
          break;
        }
        case "minute":
        case "second": {
          const kind = p.type === "minute" ? "Minute" : "Second";
          const sample = p.type === "minute" ? "4" : "5";
          if (v === digits("0" + sample)) fields.push([kind + "2"]);
          else if (v === digits(sample)) fields.push([kind]);
          else throw new Error(`${tag}: ${p.type} ${v}`);
          break;
        }
        case "dayPeriod": {
          const periods = Array.from({ length: 24 }, (_, hour) => of(at(2026, 3, 22, hour, 0, 0)));
          periods.forEach((period, hour) => {
            if (of(at(2026, 3, 22, hour, 30, 0)) !== period) throw new Error(`${tag}: the period changes within hour ${hour}`);
          });
          fields.push(["Period", nameAt(periods)]);
          break;
        }
        case "timeZoneName":
          fields.push(["Zone"]);
          break;
        default:
          throw new Error(`${tag}: a ${p.type} field`);
      }
    }
    // Adjacent text, as one.
    return fields.reduce((list, f) => {
      const last = list[list.length - 1];
      if (f[0] === "Text" && last?.[0] === "Text") last[1] += f[1];
      else list.push(f);
      return list;
    }, []);
  });

  // Zones: UTC by name, an offset as GMT+5:30 in the locale's form.
  const zone = (timeZone) => part(dtf(tag, TIME[0], timeZone).formatToParts(SAMPLE), "timeZoneName");
  const utc = zone("UTC");
  // Hours at \u0001, minutes at \u0003: whole hours east and west, then with minutes.
  const gmt = [
    zone("+10:00").replace(digits("10"), "\u0001"),
    zone("-08:00").replace(digits("8"), "\u0001"),
    zone("+05:30").replace(digits("30"), "\u0003").replace(digits("5"), "\u0001"),
    zone("-03:30").replace(digits("30"), "\u0003").replace(digits("3"), "\u0001"),
  ];
  if (gmt.some((t) => !t.includes("\u0001")) || !gmt[2].includes("\u0003") || !gmt[3].includes("\u0003")) throw new Error(`${tag}: offsets ${gmt}`);

  // Plural categories of a whole number: 0 to 199 by table; above, a multiple of a million has its
  // own (fr's "many"), and any other is as 100 + n % 100 is. Checked against Intl below.
  const pr = new Intl.PluralRules(tag);
  const plurals = Array.from({ length: 200 }, (_, n) => PLURALS.indexOf(pr.select(n)));
  const millions = PLURALS.indexOf(pr.select(1_000_000));
  // A time relative to now: for each style, unit, then past and future, a template per plural
  // category, with the number at \u0001 (empty for a category the locale does not have).
  const relative = RELATIVE_STYLES.map((style) => {
    const f = new Intl.RelativeTimeFormat(tag, { style, numeric: "always" });
    return RELATIVE_UNITS.map((unit) =>
      [-1, 1].map((sign) =>
        PLURALS.map((category) => {
          const n = plurals.indexOf(PLURALS.indexOf(category));
          const sample = n >= 0 ? n : pr.select(1_000_000) === category ? 1_000_000 : -1;
          if (sample < 0) return "";
          let out = "";
          let inNumber = false;
          for (const p of f.formatToParts(sign * sample, unit)) {
            if (["integer", "group"].includes(p.type)) {
              if (!inNumber) out += "\u0001";
              inNumber = true;
              continue;
            }
            if (p.type !== "literal") throw new Error(`${tag}: a ${p.type} in a relative time`);
            inNumber = false;
            out += p.value;
          }
          return out;
        }),
      ),
    );
  });

  return {
    tag,
    zero,
    plain,
    ofMoney,
    number,
    ofPercent,
    percent,
    money,
    symbols,
    names,
    dates,
    utc,
    gmt,
    plurals,
    millions,
    relative,
  };
}

// ---- The Rust algorithm, again, to check the data against Intl before writing it. ----

/** A whole number's plural category, by the extracted table. */
function plural(d, n) {
  return n < 200 ? d.plurals[n] : n % 1_000_000 === 0 ? d.millions : d.plurals[100 + (n % 100)];
}

/** `seconds` from now (negative: before) in a style, in the largest unit it has one of. */
function formatRelative(d, style, seconds) {
  const abs = Math.abs(seconds);
  let unit = RELATIVE_UNITS.length - 1;
  while (unit > 0 && abs < RELATIVE_SECONDS[unit]) unit--;
  const n = Math.floor(abs / RELATIVE_SECONDS[unit]);
  const template = d.relative[style][unit][seconds > 0 ? 1 : 0][plural(d, n)];
  return template.replace("\u0001", body(d, d.plain, String(n), ""));
}

const ISO_DIGITS = { BIF: 0, CLP: 0, DJF: 0, GNF: 0, ISK: 0, JPY: 0, KMF: 0, KRW: 0, PYG: 0, RWF: 0, UGX: 0, UYI: 0, VND: 0, VUV: 0, XAF: 0, XOF: 0, XPF: 0, BHD: 3, IQD: 3, JOD: 3, KWD: 3, LYD: 3, OMR: 3, TND: 3, CLF: 4, UYW: 4 };
const displayDigits = Object.fromEntries(
  Intl.supportedValuesOf("currency").map((c) => [c, new Intl.NumberFormat("en", { style: "currency", currency: c }).resolvedOptions().maximumFractionDigits]),
);

function localDigits(d, ascii) {
  const zero = d.zero.codePointAt(0);
  return [...ascii].map((c) => String.fromCodePoint(zero + (c.charCodeAt(0) - 48))).join("");
}

/** `int` and `frac`: ASCII digits; `s` the separators and grouping. */
function body(d, s, int, frac) {
  const [primary, secondary, min] = s.grouping;
  let grouped = "";
  if (int.length - primary >= min) {
    let rest = int.slice(0, int.length - primary);
    const parts = [int.slice(int.length - primary)];
    while (rest.length > secondary) {
      parts.unshift(rest.slice(rest.length - secondary));
      rest = rest.slice(0, rest.length - secondary);
    }
    parts.unshift(rest);
    grouped = parts.map((p) => localDigits(d, p)).join(s.group);
  } else grouped = localDigits(d, int);
  return frac ? grouped + s.decimal + localDigits(d, frac) : grouped;
}

/** A decimal string rounded half away from zero to `digits` fraction digits: [negative, int, frac].
 * A negative amount that rounds to zero keeps its sign, as Intl writes it (`-$0.00`). */
function round(text, digits) {
  let negative = text.startsWith("-");
  let [int, frac = ""] = text.replace(/^[-+]/, "").split(".");
  if (frac.length > digits) {
    const up = frac[digits] >= "5";
    let all = (int + frac.slice(0, digits)).split("").map(Number);
    if (up) {
      let i = all.length - 1;
      while (i >= 0 && all[i] === 9) all[i--] = 0;
      if (i < 0) all.unshift(1);
      else all[i] += 1;
    }
    const s = all.join("");
    int = s.slice(0, s.length - digits) || "0";
    frac = s.slice(s.length - digits);
  } else frac = frac.padEnd(digits, "0");
  int = int.replace(/^0+(?=\d)/, "");
  return [negative, int, frac];
}

function formatNumber(d, text) {
  const [negative, int, frac] = round(text, (text.split(".")[1] ?? "").length);
  return d.number[negative ? 1 : 0].replace("\u0001", body(d, d.plain, int, frac));
}

/** `text` (a ratio) as a percentage with at most `digits` fraction digits, without trailing zeros. */
function formatPercent(d, text, digits) {
  const negative = text.startsWith("-");
  let [int, frac = ""] = text.replace(/^[-+]/, "").split(".");
  frac = frac.padEnd(2, "0");
  const shifted = `${negative ? "-" : ""}${int}${frac.slice(0, 2)}.${frac.slice(2)}`;
  let [neg, i, f] = round(shifted.endsWith(".") ? shifted.slice(0, -1) : shifted, Math.min(digits, frac.length - 2));
  f = f.replace(/0+$/, "");
  return d.percent[neg ? 1 : 0].replace("\u0001", body(d, d.ofPercent, i, f));
}

/** `text` (a ratio) as a change: a percentage with exactly `digits` fraction digits and a sign,
 * none when it rounds to zero. */
function formatChange(d, text, digits) {
  const negative = text.startsWith("-");
  let [int, frac = ""] = text.replace(/^[-+]/, "").split(".");
  frac = frac.padEnd(2, "0");
  const shifted = `${negative ? "-" : ""}${int}${frac.slice(0, 2)}.${frac.slice(2)}`;
  const [neg, i, f] = round(shifted.endsWith(".") ? shifted.slice(0, -1) : shifted, digits);
  const zero = /^0*$/.test(i + f);
  return d.percent[zero ? 0 : neg ? 1 : 2].replace("\u0001", body(d, d.ofPercent, i, f));
}

function formatMoney(d, text, code) {
  const digits = displayDigits[code] ?? 2;
  const [negative, int, frac] = round(text, digits);
  const own = d.symbols.find(([c]) => c === code);
  const t = own ? own[negative ? 2 : 1] : d.money[negative ? 1 : 0].replace("\u0002", code);
  return t.replace("\u0001", body(d, d.ofMoney, int, frac));
}

/** The locale's minus and plus signs: where its percentage's loss and gain templates differ. */
function signs(d) {
  const [loss, gain] = [d.percent[1], d.percent[2]];
  let start = 0;
  while (loss[start] === gain[start]) start++;
  let end = 0;
  while (end < loss.length - start && loss[loss.length - 1 - end] === gain[gain.length - 1 - end]) end++;
  return [loss.slice(start, loss.length - end), gain.slice(start, gain.length - end)];
}

/** An amount with its sign, as Intl's `signDisplay: "exceptZero"`: a gain is the loss's template with
 * the plus sign in place of the minus, and what rounds to zero has none. */
function formatSignedMoney(d, text, code) {
  const digits = displayDigits[code] ?? 2;
  const [negative, int, frac] = round(text, digits);
  const own = d.symbols.find(([c]) => c === code);
  const [plain, loss] = own ? [own[1], own[2]] : [d.money[0].replace("\u0002", code), d.money[1].replace("\u0002", code)];
  const [minus, plus] = signs(d);
  if (loss.split(minus).length !== 2) throw new Error(`${d.tag} ${code}: the loss template has its minus sign other than once`);
  const t = /^0*$/.test(int + frac) ? plain : negative ? loss : loss.replace(minus, plus);
  return t.replace("\u0001", body(d, d.ofMoney, int, frac));
}

function civil(date, offset) {
  const t = new Date(date.getTime() + offset * 1000);
  return { y: t.getUTCFullYear(), mo: t.getUTCMonth() + 1, d: t.getUTCDate(), wd: t.getUTCDay(), h: t.getUTCHours(), mi: t.getUTCMinutes(), s: t.getUTCSeconds() };
}

function formatDate(d, style, date, offset) {
  const c = civil(date, offset);
  const pad = (n) => String(n).padStart(2, "0");
  let out = "";
  for (const [kind, a, b] of d.dates[style]) {
    switch (kind) {
      case "Text": out += a; break;
      case "Year": out += localDigits(d, String(c.y)); break;
      case "Year2": out += localDigits(d, pad(c.y % 100)); break;
      case "Month": out += localDigits(d, String(c.mo)); break;
      case "Month2": out += localDigits(d, pad(c.mo)); break;
      case "MonthName": out += d.names[a + c.mo - 1]; break;
      case "Day": out += localDigits(d, String(c.d)); break;
      case "Day2": out += localDigits(d, pad(c.d)); break;
      case "Weekday": out += d.names[a + c.wd]; break;
      case "Hour": {
        const h = a === 12 ? c.h % 12 || 12 : a === 11 ? c.h % 12 : a === 24 ? c.h || 24 : c.h;
        out += localDigits(d, b ? pad(h) : String(h));
        break;
      }
      case "Minute": out += localDigits(d, String(c.mi)); break;
      case "Minute2": out += localDigits(d, pad(c.mi)); break;
      case "Second": out += localDigits(d, String(c.s)); break;
      case "Second2": out += localDigits(d, pad(c.s)); break;
      case "Period": out += d.names[a + c.h]; break;
      case "Zone": {
        if (offset === 0) { out += d.utc; break; }
        const abs = Math.abs(offset);
        const minutes = Math.floor((abs % 3600) / 60);
        const t = d.gmt[(minutes ? 2 : 0) + (offset < 0 ? 1 : 0)];
        out += t.replace("\u0001", localDigits(d, String(Math.floor(abs / 3600)))).replace("\u0003", localDigits(d, pad(minutes)));
        break;
      }
    }
  }
  return out;
}

// ---- Cases: checked here, and written for the Rust test. ----

// Ratios, each written with at most 0, 1, 2 and 4 fraction digits.
const RATIOS = ["0.029", "-0.5", "1", "0", "12.345", "0.00125", "1234.5", "-0.00004", "0.0290", "0.995"];
const RATIO_DIGITS = [0, 1, 2, 4];
// A change: gains, losses, and ones that round to zero either way.
const CHANGES = [...RATIOS, "0.006", "-0.003", "0.00004", "-0.000049", "-0.00005", "0.00005"];
const CASE_CHANGES = ["0.006", "-0.003", "0", "-0.00004", "12.345", "-0.5"];
const NUMBERS = ["0", "7", "-7", "999", "1234", "12345", "123456", "1234567", "-1234567", "1234567890123", "1234.5", "-0.25", "1234567.891", "0.5"];
const CURRENCIES = ["USD", "EUR", "JPY", "GBP", "CHF", "CAD", "INR", "KWD", "BRL", "CNY", "KRW", "MXN", "PLN", "SEK", "HUF", "COP"];
const AMOUNTS = ["1234.5", "-5", "0", "21.505", "-0.004", "1234567.125", "0.995"];
// The ones the Rust test formats again (this script checks every currency).
const CASE_CURRENCIES = ["USD", "EUR", "JPY", "CHF", "KWD", "INR", "BRL", "CAD"];
const CASE_AMOUNTS = ["1234.5", "-5", "21.505", "-0.004", "1234567.125"];
// An amount with its sign: gains, losses, and ones that round to zero either way.
const SIGNED_AMOUNTS = [...AMOUNTS, "0.004", "-0.005", "3.2", "-1.23"];
const CASE_SIGNED = ["3.2", "-1.23", "0", "-0.004", "1234567.125"];
const INSTANTS = [
  SAMPLE,
  at(1999, 12, 31, 0, 30, 9),
  at(2005, 1, 5, 12, 0, 0),
  at(2026, 7, 4, 23, 59, 59),
  ...Array.from({ length: 12 }, (_, m) => at(2026, m + 1, 10 + m, 9, 7, 0)),
  ...Array.from({ length: 24 }, (_, h) => at(2026, 3, 22, h, 30, 0)),
];
// Seconds from now: before (negative) and after, each unit and its edges.
const RELATIVE_CASES = [0, -1, 1, 2, -2, 5, -21, 59, -59, 60, -61, 119, 120, -3599, 3600, -3601, 7200, 38_100, -86_399, 86_400, -86_401, 172_800, 950_400, -2_592_000, 31_536_000_00];
const OFFSETS = [["+05:30", 19800], ["-08:00", -28800], ["+10:00", 36000], ["-03:30", -12600], ["+13:45", 49500]];

const datas = [];
const aliases = [];
const cases = [];
let checked = 0;
const expect = (tag, kind, input, ours, intl) => {
  checked += 1;
  if (ours !== intl) throw new Error(`${tag} ${kind} ${input}: we write ${JSON.stringify(ours)}, Intl ${JSON.stringify(intl)}`);
};
for (const tag of [...LANGUAGES, ...REGIONS]) {
  const d = localeData(tag);
  const forCases = [];
  for (const n of NUMBERS) {
    const intl = new Intl.NumberFormat(tag, { maximumFractionDigits: 20 }).format(n);
    expect(tag, "number", n, formatNumber(d, n), intl);
    if (n.length !== 3 && n !== "7" && n !== "0") forCases.push(["number", n, intl]);
  }
  for (const r of RATIOS) {
    for (const digits of RATIO_DIGITS) {
      const intl = new Intl.NumberFormat(tag, { style: "percent", maximumFractionDigits: digits }).format(r);
      expect(tag, `percent ${digits}`, r, formatPercent(d, r, digits), intl);
      if (digits === 2 || r === "12.345") forCases.push([`percent ${digits}`, r, intl]);
    }
  }
  for (const r of CHANGES) {
    for (const digits of RATIO_DIGITS) {
      const options = { style: "percent", minimumFractionDigits: digits, maximumFractionDigits: digits, signDisplay: "exceptZero" };
      const intl = new Intl.NumberFormat(tag, options).format(r);
      expect(tag, `change ${digits}`, r, formatChange(d, r, digits), intl);
      if (digits === 2 && CASE_CHANGES.includes(r)) forCases.push([`change ${digits}`, r, intl]);
    }
  }
  for (const code of Intl.supportedValuesOf("currency")) {
    for (const amount of CURRENCIES.includes(code) ? AMOUNTS : ["1234.5", "-5"]) {
      const intl = new Intl.NumberFormat(tag, { style: "currency", currency: code }).format(amount);
      expect(tag, "money", `${amount} ${code}`, formatMoney(d, amount, code), intl);
      if (CASE_CURRENCIES.includes(code) && CASE_AMOUNTS.includes(amount)) forCases.push(["money", `${amount} ${code}`, intl]);
    }
    for (const amount of CURRENCIES.includes(code) ? SIGNED_AMOUNTS : ["1234.5", "-5", "0"]) {
      const intl = new Intl.NumberFormat(tag, { style: "currency", currency: code, signDisplay: "exceptZero" }).format(amount);
      expect(tag, "signed_money", `${amount} ${code}`, formatSignedMoney(d, amount, code), intl);
      if (CASE_CURRENCIES.includes(code) && CASE_SIGNED.includes(amount)) forCases.push(["signed_money", `${amount} ${code}`, intl]);
    }
  }
  BAGS.forEach((bag, style) => {
    for (const date of INSTANTS) {
      const intl = dtf(tag, bag).format(date);
      expect(tag, `date ${style}`, date.toISOString(), formatDate(d, style, date, 0), intl);
      const hour = INSTANTS.indexOf(date) - 16;
      if (date === SAMPLE || date === INSTANTS[1] || (style === 12 && [0, 5, 11, 12, 13, 17, 20, 23].includes(hour))) forCases.push([`date ${style}`, date.toISOString(), intl]);
    }
    if (bag.timeZoneName) {
      for (const [zone, offset] of OFFSETS) {
        const intl = dtf(tag, bag, zone).format(SAMPLE);
        expect(tag, `date ${style} ${zone}`, SAMPLE.toISOString(), formatDate(d, style, SAMPLE, offset), intl);
        if (style === 4) forCases.push([`date ${style} ${offset}`, SAMPLE.toISOString(), intl]);
      }
    }
  });
  RELATIVE_STYLES.forEach((style, s) => {
    const f = new Intl.RelativeTimeFormat(tag, { style, numeric: "always" });
    for (const seconds of RELATIVE_CASES) {
      const abs = Math.abs(seconds);
      let unit = RELATIVE_UNITS.length - 1;
      while (unit > 0 && abs < RELATIVE_SECONDS[unit]) unit--;
      const n = Math.floor(abs / RELATIVE_SECONDS[unit]);
      const intl = f.format(seconds > 0 ? n : -n, RELATIVE_UNITS[unit]);
      expect(tag, `relative ${s}`, String(seconds), formatRelative(d, s, seconds), intl);
      if (RELATIVE_CASES.indexOf(seconds) % 3 === 0 || s === 0) forCases.push([`relative ${s}`, String(seconds), intl]);
    }
    // Whole numbers from 1 to 300 000, and multiples of a million, in days; to 300 in each of
    // the smaller units, which have their own words.
    for (const n of [...Array.from({ length: 3000 }, (_, i) => i + 1), ...Array.from({ length: 300 }, (_, i) => 3000 + i * 997), 1e6, 2e6, 1e6 + 1, 1e7, 21e6]) {
      expect(tag, `relative ${s} days`, String(n), formatRelative(d, s, n * 86400), f.format(n, "day"));
    }
    for (let n = 1; n <= 300; n++) {
      for (const [u, unit] of RELATIVE_UNITS.slice(0, 3).entries()) {
        for (const sign of [-1, 1]) {
          // Below the next unit's size it is written in this one (seconds to 59, and so on); past
          // it, the template is the same one the cases reach, so it is checked directly.
          const template = d.relative[s][u][sign > 0 ? 1 : 0][plural(d, n)];
          const ours = template.replace("\u0001", body(d, d.plain, String(n), ""));
          expect(tag, `relative ${s} ${unit}s`, String(sign * n), ours, f.format(sign * n, unit));
        }
      }
    }
  });
  // A region with its language's data is an alias.
  const language = tag.split("-")[0];
  const same = tag.includes("-") && datas.find((x) => x.tag === language && JSON.stringify({ ...x, tag: 0 }) === JSON.stringify({ ...d, tag: 0 }));
  if (same) aliases.push([tag, language]);
  else {
    datas.push(d);
    for (const [kind, input, intl] of forCases) cases.push([tag, kind, input, intl]);
  }
}

// ---- Output. ----

const bad = /[<>&"]/;
const rust = (s) => {
  if (bad.test(s)) throw new Error(`locale text ${JSON.stringify(s)} needs escaping`);
  let out = '"';
  for (const c of s) {
    const code = c.codePointAt(0);
    if (c === "\\") out += "\\\\";
    // Controls, format characters (marks, joiners, zero-width spaces) and spaces other than " ".
    else if (code < 0x20 || (code >= 0x7f && code < 0xa0) || (c !== " " && /[\p{Cf}\p{Z}]/u.test(c))) out += `\\u{${code.toString(16)}}`;
    else out += c;
  }
  return out + '"';
};
const field = ([kind, a, b]) =>
  kind === "Text" ? `F::Text(${rust(a)})` : kind === "Hour" ? `F::Hour(${a}, ${b})` : a === undefined ? `F::${kind}` : `F::${kind}(${a})`;
const seps = (s) => `::corten::i18n::__private::Separators { decimal: ${rust(s.decimal)}, group: ${rust(s.group)}, grouping: [${s.grouping.join(", ")}] }`;
const tokens = (d) =>
  [
    `::corten::i18n::__private::LocaleData { tag: ${rust(d.tag)}, zero: '${d.zero}', plain: ${seps(d.plain)}, money_separators: ${seps(d.ofMoney)}, percent_separators: ${seps(d.ofPercent)},`,
    ` number: [${d.number.map(rust).join(", ")}], percent: [${d.percent.map(rust).join(", ")}], money: [${d.money.map(rust).join(", ")}],`,
    ` currencies: &[${d.symbols.map(([c, p, n]) => `(${rust(c)}, ${rust(p)}, ${rust(n)})`).join(", ")}],`,
    ` names: &[${d.names.map(rust).join(", ")}],`,
    ` dates: { use ::corten::i18n::__private::Field as F; [${d.dates.map((fs) => `&[${fs.map(field).join(", ")}]`).join(", ")}] },`,
    ` utc: ${rust(d.utc)}, gmt: [${d.gmt.map(rust).join(", ")}],`,
    ` plurals: [${d.plurals.join(", ")}], millions: ${d.millions},`,
    ` relative: [${d.relative.map((units) => `[${units.map((signs) => `[${signs.map((cats) => `[${cats.map(rust).join(", ")}]`).join(", ")}]`).join(", ")}]`).join(", ")}] }`,
  ].join("");
const source = (s) => "r#\"" + s.replace(/"#/g, '"\\#') + "\"#";
const header = `// Generated by scripts/locale-data.mjs from Node ${process.version}'s Intl (ICU ${process.versions.icu}, CLDR ${process.versions.cldr}). Do not edit.`;

writeFileSync(
  join(root, "crates/corten-macros/src/locale_data.rs"),
  `${header}
//! Number, currency and date data for each locale \`messages!\` can list, as the tokens of a
//! \`corten::i18n::__private::LocaleData\`, and the tags that use another's data.

/// (tag, tokens), by tag.
pub const LOCALES: &[(&str, &str)] = &[
${datas.map((d) => `    (${rust(d.tag)}, ${source(tokens(d))}),`).join("\n")}
];

/// (tag, the tag whose data it has).
pub const ALIASES: &[(&str, &str)] = &[
${aliases.map(([a, b]) => `    (${rust(a)}, ${rust(b)}),`).join("\n")}
];
`,
);

const display = Object.entries(displayDigits).filter(([, n]) => n !== 2);
writeFileSync(
  join(root, "crates/corten/src/i18n/display_digits.rs"),
  `${header}

/// The fraction digits an amount in the currency is shown with, where it is not 2, by code.
pub(super) const DISPLAY_DIGITS: &[(&str, u8)] = &[
${display.map(([c, n]) => `    ("${c}", ${n}),`).join("\n")}
];
`,
);

writeFileSync(
  join(root, "crates/corten/src/i18n/cases.tsv"),
  `# ${header.slice(3)}\n# locale\tcase\tinput\twhat Intl writes\n` + cases.map((c) => c.join("\t")).join("\n") + "\n",
);
console.log(`${datas.length} locales (${aliases.length} aliases), ${checked} values checked against Intl, ${cases.length} test cases`);