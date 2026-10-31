// @ts-check
// Numbers, money and dates for a script, in the page's locale: what a `%{..}` argument of
// `data-on` becomes (`t.js().money(USD)` in the markup). The server writes the locale's pattern
// with the argument, so the browser writes exactly what the server does (the same algorithm as
// crates/corten/src/i18n/format.rs; scripts/format.mjs checks the two against each other). No DOM
// here. Built into dist/format.js, a module of its own that Corten serves with the app's scripts
// (`corten-format`), so only a page whose handlers take a format loads it.

/**
 * A format as the server writes it. `z` is the locale's zero; `s` its decimal and group
 * separators and grouping (primary, secondary, fewest digits grouped); `t` templates, positive and
 * negative, with the digits at \u0001, and a change's or signed amount's third, a gain's (`+5%`,
 * `+$3.20`); `d` an amount's
 * fraction digits; `p` a percentage's most fraction digits (it takes a ratio), a change's exact
 * ones; `f` a date's fields; `o`
 * the zone's offset in seconds east, and for a pattern that shows the zone, `u` UTC's name and `g`
 * an offset's templates (hours at \u0001, minutes at \u0003: whole hours east, west, then with
 * minutes). A period (`P`) is runs of a name and the hour it ends before: ["P","AM",12,"PM",24].
 * A plural message (`t.js().cards()`) is a number's format with `c`, the plural category (0 to 5:
 * zero, one, two, few, many, other) of each whole number to 199 and then of a multiple of a
 * million, and `m`, the message's form for each category (text and argument names alternating,
 * as a message's), `null` for one the locale does not use.
 * @typedef {{ z: string, s?: [string, string, number, number, number], t?: [string, string] | [string, string, string], d?: number, p?: number, f?: Field[], o?: number, u?: string, g?: string[], c?: string, m?: (string[] | null)[] }} Spec
 * @typedef {[string, ...(string | number)[]]} Field
 */

/**
 * The function a format argument is.
 * @param {Spec} spec
 * @returns {(value: any, offset?: number) => string}
 */
export function formatter(spec) {
  if (spec.m) return plural(spec, /** @type {string} */ (spec.c), spec.m);
  if (spec.f) return (at, offset) => date(spec, at, offset);
  if (spec.d !== undefined) {
    const d = spec.d;
    return (minor) => {
      // Rounded half away from zero, as the server rounds: 19.99 * 100 is 1998.9999999999998.
      const text = String(Math.round(Math.abs(minor))).padStart(d + 1, "0");
      const negative = minor < 0 || Object.is(minor, -0);
      // A signed amount: no sign on what rounds to zero, either way.
      const at = spec.t?.length === 3 ? (/[1-9]/.test(text) ? (negative ? 1 : 2) : 0) : negative ? 1 : 0;
      return number(spec, at, text.slice(0, text.length - d), text.slice(text.length - d));
    };
  }
  const p = spec.p;
  return (n) => {
    const m = /^([-+]?)(\d*)(?:\.(\d+))?$/.exec(typeof n === "number" ? plain(n) : n);
    if (!m || !(m[2] || m[3])) return String(n);
    if (p === undefined) return number(spec, m[1] === "-" ? 1 : 0, m[2].replace(/^0+(?=\d)/, "") || "0", m[3] ?? "");
    // A percentage: the ratio Ã— 100, moving the point in the text (0.029 * 100 is
    // 2.9000000000000004), then at most `p` fraction digits, rounded half away from zero.
    let f = (m[3] ?? "").padEnd(2, "0");
    let i = m[2] + f.slice(0, 2);
    f = f.slice(2);
    const change = spec.t?.length === 3;
    // A change has at most 38 digits in all, as the server's Decimal: the whole part's, then the
    // fraction's.
    const q = change ? Math.max(0, Math.min(p, 38 - (i.replace(/^0+(?=\d)/, "") || "0").length)) : p;
    if (f.length > q) {
      const s = String(BigInt(i + f.slice(0, q)) + BigInt(f[q] > "4")).padStart(q + 1, "0");
      [i, f] = [s.slice(0, s.length - q), s.slice(s.length - q)];
    }
    i = i.replace(/^0+(?=\d)/, "");
    if (!change) return number(spec, m[1] === "-" ? 1 : 0, i, f.replace(/0+$/, ""));
    // A change: exactly its digits, and a sign unless it rounds to zero.
    f = f.padEnd(q, "0");
    return number(spec, /[1-9]/.test(i + f) ? (m[1] === "-" ? 1 : 2) : 0, i, f);
  };
}

/**
 * A number as decimal text with no exponent, the digits JavaScript prints for it: `1e-7` is
 * "0.0000001" and `1e21` "1000000000000000000000", so a ratio a script works out is written, not
 * passed through as "1e-7". @param {number} n
 */
function plain(n) {
  const s = String(n);
  const e = s.indexOf("e");
  if (e < 0) return s;
  const [, sign, int, frac = ""] = /** @type {RegExpExecArray} */ (/^(-?)(\d+)(?:\.(\d+))?$/.exec(s.slice(0, e)));
  const all = int + frac;
  const point = int.length + Number(s.slice(e + 1));
  if (point <= 0) return `${sign}0.${"0".repeat(-point)}${all}`;
  if (point >= all.length) return sign + all + "0".repeat(point - all.length);
  return `${sign}${all.slice(0, point)}.${all.slice(point)}`;
}

/** ASCII digits in the locale's. @param {string} zero @param {string} ascii */
const digits = (zero, ascii) => {
  if (zero === "0") return ascii;
  const z = /** @type {number} */ (zero.codePointAt(0)) - 48;
  return ascii.replace(/\d/g, (c) => String.fromCodePoint(z + c.charCodeAt(0)));
};

/** `t`'s template `at` (positive, negative, a change's gain). @param {Spec} spec @param {number} at @param {string} int @param {string} frac */
function number(spec, at, int, frac) {
  const [decimal, group, primary, secondary, min] = /** @type {NonNullable<Spec["s"]>} */ (spec.s);
  let body = int;
  if (int.length >= primary + min) {
    body = int.slice(-primary);
    let rest = int.slice(0, -primary);
    for (; rest.length > secondary; rest = rest.slice(0, -secondary)) body = rest.slice(-secondary) + group + body;
    body = rest + group + body;
  }
  body = digits(spec.z, body);
  if (frac) body += decimal + digits(spec.z, frac);
  return /** @type {NonNullable<Spec["t"]>} */ (spec.t)[at].replace("\u0001", () => body);
}

/**
 * A plural message: the form for `count`'s category, picked as the server picks it (a whole
 * number, of any size, by the table; anything else, as a fraction, by `other`), with `count`
 * written as `t.number` writes it when it is a number.
 * @param {Spec} spec @param {string} table @param {(string[] | null)[]} forms
 * @returns {(args: Record<string, string | number>) => string}
 */
function plural(spec, table, forms) {
  const write = formatter({ z: spec.z, s: spec.s, t: spec.t });
  return (args) => {
    const n = Number(args.count);
    const whole = typeof args.count === "number" ? Number.isInteger(n) : /^-?\d+$/.test(args.count);
    // Exactly, past 2^53 too: a large count's category depends on its last digits.
    const a = whole ? BigInt(args.count) * (String(args.count).startsWith("-") ? -1n : 1n) : 0n;
    const at = whole ? Number(table[a < 200n ? Number(a) : a % 1_000_000n === 0n ? 200 : 100 + Number(a % 100n)]) : 5;
    const parts = forms[at] ?? /** @type {string[]} */ (forms[5]);
    let text = parts[0];
    for (let i = 1; i < parts.length; i += 2) {
      const name = parts[i];
      text += (name === "count" && Number.isFinite(n) && typeof args.count === "number" ? write(n) : String(args[name])) + parts[i + 1];
    }
    return text;
  };
}

/**
 * @param {Spec} spec @param {Date | number | string} at
 * @param {number} [east] the offset to write it at, in seconds east, instead of the format's
 */
function date(spec, at, east) {
  const offset = east ?? spec.o ?? 0;
  const t = new Date(new Date(at).getTime() + offset * 1000);
  if (Number.isNaN(t.getTime())) return String(at);
  const [y, mo, d, h, mi, s] = [t.getUTCFullYear(), t.getUTCMonth(), t.getUTCDate(), t.getUTCHours(), t.getUTCMinutes(), t.getUTCSeconds()];
  /** @param {number} n @param {unknown} [two] */
  const n = (n, two) => digits(spec.z, two ? String(n).padStart(2, "0") : String(n));
  let out = "";
  for (const [kind, ...a] of /** @type {Field[]} */ (spec.f)) {
    out +=
      kind === "" ? a[0]
      : kind === "y" ? n(y)
      : kind === "yy" ? n(((y % 100) + 100) % 100, 1)
      : kind === "M" || kind === "MM" ? n(mo + 1, kind === "MM")
      : kind === "N" ? a[mo]
      : kind === "d" || kind === "dd" ? n(d, kind === "dd")
      : kind === "W" ? a[t.getUTCDay()]
      : kind === "h" ? n(a[0] === 12 ? h % 12 || 12 : a[0] === 11 ? h % 12 : a[0] === 24 ? h || 24 : h, a[1])
      : kind === "m" || kind === "mm" ? n(mi, kind === "mm")
      : kind === "s" || kind === "ss" ? n(s, kind === "ss")
      : kind === "P" ? period(a, h)
      : zone(spec, offset, n);
  }
  return out;
}

/** The name of hour `h`'s period: runs of a name and the hour it ends before. @param {(string | number)[]} runs @param {number} h */
function period(runs, h) {
  let i = 0;
  while (h >= /** @type {number} */ (runs[i + 1])) i += 2;
  return /** @type {string} */ (runs[i]);
}

/** @param {Spec} spec @param {number} offset @param {(n: number, two?: unknown) => string} n */
function zone(spec, offset, n) {
  if (!offset) return /** @type {string} */ (spec.u);
  const east = Math.abs(offset);
  const minutes = Math.floor((east % 3600) / 60);
  const template = /** @type {string[]} */ (spec.g)[(minutes ? 2 : 0) + (offset < 0 ? 1 : 0)];
  return template.replace("\u0001", () => n(Math.floor(east / 3600))).replace("\u0003", () => n(minutes, 1));
}