// Writes the number, currency and date data `messages!` puts into an app for each locale it
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

// The styles, in the order of LocaleData::dates: four dates, each date with each time, two times.
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
const BAGS = [...DATE, ...DATE.flatMap((d) => TIME.map((t) => ({ ...d, ...t }))), ...TIME];

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
  const separators = (f) => {
    const np = f.formatToParts(-1234567.5);
    const groups = f.formatToParts(1234567890).filter((p) => p.type === "integer").map((p) => p.value.length);
    const primary = groups[groups.length - 1];
    const secondary = groups.length > 2 ? groups[groups.length - 2] : primary;
    const grouped = (v) => f.formatToParts(v).some((p) => p.type === "group");
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

  return {
    tag,
    zero,
    plain,
    ofMoney,
    number,
    money,
    symbols,
    names,
    dates,
    utc,
    gmt,
  };
}

// ---- The Rust algorithm, again, to check the data against Intl before writing it. ----

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

function formatMoney(d, text, code) {
  const digits = displayDigits[code] ?? 2;
  const [negative, int, frac] = round(text, digits);
  const own = d.symbols.find(([c]) => c === code);
  const t = own ? own[negative ? 2 : 1] : d.money[negative ? 1 : 0].replace("\u0002", code);
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

const NUMBERS = ["0", "7", "-7", "999", "1234", "12345", "123456", "1234567", "-1234567", "1234567890123", "1234.5", "-0.25", "1234567.891", "0.5"];
const CURRENCIES = ["USD", "EUR", "JPY", "GBP", "CHF", "CAD", "INR", "KWD", "BRL", "CNY", "KRW", "MXN", "PLN", "SEK", "HUF", "COP"];
const AMOUNTS = ["1234.5", "-5", "0", "21.505", "-0.004", "1234567.125", "0.995"];
// The ones the Rust test formats again (this script checks every currency).
const CASE_CURRENCIES = ["USD", "EUR", "JPY", "CHF", "KWD", "INR", "BRL", "CAD"];
const CASE_AMOUNTS = ["1234.5", "-5", "21.505", "-0.004", "1234567.125"];
const INSTANTS = [
  SAMPLE,
  at(1999, 12, 31, 0, 30, 9),
  at(2005, 1, 5, 12, 0, 0),
  at(2026, 7, 4, 23, 59, 59),
  ...Array.from({ length: 12 }, (_, m) => at(2026, m + 1, 10 + m, 9, 7, 0)),
  ...Array.from({ length: 24 }, (_, h) => at(2026, 3, 22, h, 30, 0)),
];
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
  for (const code of Intl.supportedValuesOf("currency")) {
    for (const amount of CURRENCIES.includes(code) ? AMOUNTS : ["1234.5", "-5"]) {
      const intl = new Intl.NumberFormat(tag, { style: "currency", currency: code }).format(amount);
      expect(tag, "money", `${amount} ${code}`, formatMoney(d, amount, code), intl);
      if (CASE_CURRENCIES.includes(code) && CASE_AMOUNTS.includes(amount)) forCases.push(["money", `${amount} ${code}`, intl]);
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
    `::corten::i18n::__private::LocaleData { tag: ${rust(d.tag)}, zero: '${d.zero}', plain: ${seps(d.plain)}, money_separators: ${seps(d.ofMoney)},`,
    ` number: [${d.number.map(rust).join(", ")}], money: [${d.money.map(rust).join(", ")}],`,
    ` currencies: &[${d.symbols.map(([c, p, n]) => `(${rust(c)}, ${rust(p)}, ${rust(n)})`).join(", ")}],`,
    ` names: &[${d.names.map(rust).join(", ")}],`,
    ` dates: { use ::corten::i18n::__private::Field as F; [${d.dates.map((fs) => `&[${fs.map(field).join(", ")}]`).join(", ")}] },`,
    ` utc: ${rust(d.utc)}, gmt: [${d.gmt.map(rust).join(", ")}] }`,
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