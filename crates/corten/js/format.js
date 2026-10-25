// @ts-check
// Numbers, money and dates for a script, in the page's locale: what a `%{..}` argument of
// `data-on` becomes (`t.js().money(USD)` in the markup). The server writes the locale's pattern
// with the argument, so the browser writes exactly what the server does (the same algorithm as
// crates/corten/src/i18n/format.rs; scripts/format.mjs checks the two against each other). No DOM
// here. Bundled into dist/corten.js.

/**
 * A format as the server writes it. `z` is the locale's zero; `s` its decimal and group
 * separators and grouping (primary, secondary, fewest digits grouped); `t` templates, positive and
 * negative, with the digits at \u0001; `d` an amount's fraction digits; `f` a date's fields; `o`
 * the zone's offset in seconds east, and for a pattern that shows the zone, `u` UTC's name and `g`
 * an offset's templates (hours at \u0001, minutes at \u0003: whole hours east, west, then with
 * minutes). A period (`P`) is runs of a name and the hour it ends before: ["P","AM",12,"PM",24].
 * @typedef {{ z: string, s?: [string, string, number, number, number], t?: [string, string], d?: number, f?: Field[], o?: number, u?: string, g?: string[] }} Spec
 * @typedef {[string, ...(string | number)[]]} Field
 */

/**
 * The function a format argument is.
 * @param {Spec} spec
 * @returns {(value: any) => string}
 */
export function formatter(spec) {
  if (spec.f) return (at) => date(spec, at);
  if (spec.d !== undefined) {
    const d = spec.d;
    return (minor) => {
      // Rounded half away from zero, as the server rounds: 19.99 * 100 is 1998.9999999999998.
      const text = String(Math.round(Math.abs(minor))).padStart(d + 1, "0");
      return number(spec, minor < 0 || Object.is(minor, -0), text.slice(0, text.length - d), text.slice(text.length - d));
    };
  }
  return (n) => {
    const m = /^([-+]?)(\d*)(?:\.(\d+))?$/.exec(typeof n === "number" ? (Number.isInteger(n) ? n.toFixed(0) : String(n)) : n);
    if (!m || !(m[2] || m[3])) return String(n);
    return number(spec, m[1] === "-", m[2].replace(/^0+(?=\d)/, "") || "0", m[3] ?? "");
  };
}

/** ASCII digits in the locale's. @param {string} zero @param {string} ascii */
const digits = (zero, ascii) => {
  if (zero === "0") return ascii;
  const z = /** @type {number} */ (zero.codePointAt(0)) - 48;
  return ascii.replace(/\d/g, (c) => String.fromCodePoint(z + c.charCodeAt(0)));
};

/** @param {Spec} spec @param {boolean} negative @param {string} int @param {string} frac */
function number(spec, negative, int, frac) {
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
  return /** @type {NonNullable<Spec["t"]>} */ (spec.t)[negative ? 1 : 0].replace("\u0001", () => body);
}

/** @param {Spec} spec @param {Date | number | string} at */
function date(spec, at) {
  const offset = spec.o ?? 0;
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