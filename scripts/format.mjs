// Checks the browser's formats (crates/corten/js/format.js) against the server's: each case
// Intl wrote (crates/corten/src/i18n/cases.tsv, which i18n.rs checks the Rust side against), run
// with the format the server sends a script for it (src/i18n/specs.tsv, written by i18n.rs).
// Run by npm run check.
import { readFileSync } from "node:fs";
import { formatter } from "../crates/corten/js/format.js";

const rows = (file) =>
  readFileSync(new URL(`../crates/corten/src/i18n/${file}`, import.meta.url), "utf8")
    .split("\n")
    .filter((line) => line && !line.startsWith("#"))
    .map((line) => line.split("\t"));
const specs = new Map(rows("specs.tsv").map(([tag, key, json]) => [`${tag} ${key}`, JSON.parse(json)]));

/** An amount in minor units, or null when it has more digits than the currency shows (rounding it is the server's). @param {string} amount @param {number} d */
function minor(amount, d) {
  const [, sign, int, frac = ""] = /** @type {RegExpExecArray} */ (/^(-?)(\d+)(?:\.(\d+))?$/.exec(amount));
  if (frac.length > d) return null;
  return Number(sign + int + frac.padEnd(d, "0"));
}

let failed = 0;
let checked = 0;
const check = (what, got, expected) => {
  checked += 1;
  if (got !== expected) {
    failed += 1;
    if (failed <= 20) console.error(`format: ${what}\n  expected: ${JSON.stringify(expected)}\n  got:      ${JSON.stringify(got)}`);
  }
};
for (const [tag, kind, input, expected] of rows("cases.tsv")) {
  const [what, style, offset] = kind.split(" ");
  // A relative time is not a script's format: RelativeTime has the browser's Intl write it, and
  // locale-data.mjs checked the server's data against Intl.
  if (what === "relative") continue;
  if (what === "number") {
    const format = formatter(specs.get(`${tag} number`));
    check(`${tag} number ${input}`, format(input), expected);
    // A number the script computed, when it is the same value.
    if (String(Number(input)) === input) check(`${tag} number ${input} (a number)`, format(Number(input)), expected);
  } else if (what === "percent" || what === "change") {
    const format = formatter(specs.get(`${tag} ${what} ${style}`));
    check(`${tag} ${what} ${style} ${input}`, format(input), expected);
    if (String(Number(input)) === input) check(`${tag} ${what} ${style} ${input} (a number)`, format(Number(input)), expected);
  } else if (what === "money" || what === "signed_money") {
    const [amount, code] = input.split(" ");
    const spec = specs.get(`${tag} ${what} ${code}`);
    const cents = minor(amount, /** @type {number} */ (spec.d));
    if (cents !== null) check(`${tag} money ${input}`, formatter(spec)(cents), expected);
  } else {
    const spec = { ...specs.get(`${tag} date ${style}`), o: Number(offset ?? 0) };
    check(`${tag} ${kind} ${input}`, formatter(spec)(input), expected);
    check(`${tag} ${kind} ${input} (a Date)`, formatter(spec)(new Date(input)), expected);
  }
}
// A number a script works out may be one JavaScript prints with an exponent; it is written as its
// digits. And a change keeps at most 38 digits in all, as the server's Decimal does.
const change = formatter(specs.get("en change 2"));
check("a change of 1e-7", change(1e-7), "0.00%");
check("a change of -1e-7", change(-1e-7), "0.00%");
check("a change of 0.00006", change(6e-5), "+0.01%");
check("a percentage of 1e21", formatter(specs.get("en percent 0"))(1e21), "100,000,000,000,000,000,000,000%");
check("a change with more digits than fit", formatter({ ...specs.get("en change 2"), p: 40 })("0.006"), `+0.${"6".padEnd(37, "0")}%`);
// A signed amount a script works out in minor units: what rounds to zero has no sign either way,
// as the server's cases with more digits than the currency shows (`-0.004 USD`) say.
const signed = formatter(specs.get("en signed_money USD"));
check("a signed amount of -0.4 cents", signed(-0.4), "$0.00");
check("a signed amount of -0", signed(-0), "$0.00");
check("a signed amount of 0.4 cents", signed(0.4), "$0.00");
check("a signed amount of -0.6 cents", signed(-0.6), "-$0.01");
check("a signed amount of 320 cents", signed(320), "+$3.20");
check("a signed amount in fr", formatter(specs.get("fr signed_money EUR"))(320), "+3,20\u00a0€");
console.log(`format: ${checked - failed} of ${checked} cases write what the server writes`);
process.exit(failed || checked < 9000 ? 1 : 0);
